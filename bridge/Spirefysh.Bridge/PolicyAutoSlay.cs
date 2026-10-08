using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Godot;
using HarmonyLib;
using MegaCrit.Sts2.Core.AutoSlay.Handlers.Rooms;
using MegaCrit.Sts2.Core.AutoSlay.Helpers;
using MegaCrit.Sts2.Core.Combat;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.Entities.Merchant;
using MegaCrit.Sts2.Core.Entities.Players;
using MegaCrit.Sts2.Core.GameActions;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Models.Events;
using MegaCrit.Sts2.Core.Nodes;
using MegaCrit.Sts2.Core.Nodes.Cards.Holders;
using MegaCrit.Sts2.Core.Nodes.CommonUi;
using MegaCrit.Sts2.Core.Nodes.Events;
using MegaCrit.Sts2.Core.Nodes.GodotExtensions;
using MegaCrit.Sts2.Core.Nodes.RestSite;
using MegaCrit.Sts2.Core.Nodes.Rewards;
using MegaCrit.Sts2.Core.Nodes.Screens.CharacterSelect;
using MegaCrit.Sts2.Core.Nodes.Screens.Map;
using MegaCrit.Sts2.Core.Nodes.Screens.Shops;
using MegaCrit.Sts2.Core.Nodes.Screens.TreasureRoomRelic;
using MegaCrit.Sts2.Core.Random;
using MegaCrit.Sts2.Core.Runs;

namespace Spirefysh.Bridge;

internal sealed record PolicyChoice(
    int Index,
    string Kind,
    string? SourceId,
    string? TargetId,
    string? ModelId,
    string? Error = null);

internal sealed class PolicyClient : IDisposable
{
    private readonly Process _process;
    private readonly object _gate = new();

    private PolicyClient(Process process) => _process = process;

    internal static PolicyClient? CreateFromConfiguration()
    {
        var checkpoint = System.Environment.GetEnvironmentVariable("SPIREFYSH_POLICY_CHECKPOINT");
        if (string.IsNullOrWhiteSpace(checkpoint)) return null;
        checkpoint = Path.GetFullPath(checkpoint);
        var directory = Path.GetDirectoryName(typeof(PolicyClient).Assembly.Location)!;
        var config = JsonSerializer.Deserialize<AdvisorConfiguration>(
            File.ReadAllText(Path.Combine(directory, "Spirefysh.Bridge.advisor-config")),
            TraceJson.Options) ?? throw new InvalidDataException("Invalid advisor configuration.");
        var python = Path.GetFullPath(config.Python);
        var script = Path.Combine(directory, "policy.py");
        foreach (var path in new[] { python, script, checkpoint })
            if (!File.Exists(path)) throw new FileNotFoundException("Policy dependency is missing.", path);
        var start = new ProcessStartInfo(python)
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetTempPath()
        };
        start.ArgumentList.Add("-u");
        start.ArgumentList.Add(script);
        start.ArgumentList.Add(checkpoint);
        start.Environment["PYTHONPATH"] = System.Environment.GetEnvironmentVariable(
            "SPIREFYSH_POLICY_PYTHONPATH") ?? throw new InvalidOperationException(
            "SPIREFYSH_POLICY_PYTHONPATH is required with SPIREFYSH_POLICY_CHECKPOINT.");
        var process = Process.Start(start)
            ?? throw new InvalidOperationException("Could not start the policy process.");
        process.ErrorDataReceived += (_, args) =>
        {
            if (!string.IsNullOrWhiteSpace(args.Data))
                Console.Error.WriteLine($"[Spirefysh policy] {args.Data}");
        };
        process.BeginErrorReadLine();
        return new(process);
    }

    internal PolicyChoice Choose(CapturedSnapshot snapshot)
    {
        var request = JsonSerializer.Serialize(new
        {
            before = snapshot.Public,
            oracle_before = snapshot.Oracle,
            allowed_kinds = snapshot.Public.Players[0].LegalActions
                .Select(action => action.Kind).Concat(snapshot.Public.VisibleOffers.Select(action => action.Kind))
                .Distinct().ToArray(),
            allowed_models = snapshot.Public.Players[0].LegalActions
                .Select(action => ModelId(snapshot.Public.Players[0], action))
                .OfType<string>()
                .Distinct().ToArray(),
            allowed_targets = snapshot.Public.Players[0].LegalActions
                .Concat(snapshot.Public.VisibleOffers)
                .Where(action => action.Kind == "map_node")
                .Select(action => action.TargetId).OfType<string>().Distinct().ToArray()
        }, TraceJson.Options);
        lock (_gate)
        {
            if (_process.HasExited) throw new InvalidOperationException("policy process exited");
            _process.StandardInput.WriteLine(request);
            _process.StandardInput.Flush();
            var response = _process.StandardOutput.ReadLineAsync().WaitAsync(TimeSpan.FromSeconds(30))
                .GetAwaiter().GetResult() ?? throw new EndOfStreamException("policy process closed its output");
            var choice = JsonSerializer.Deserialize<PolicyChoice>(response, TraceJson.Options)
                ?? throw new InvalidDataException("policy returned an empty response");
            if (choice.Error is not null) throw new InvalidOperationException(choice.Error);
            return choice;
        }
    }

    private static string? ModelId(PlayerSnapshot player, SemanticAction action)
    {
        if (action.Kind == "play_card")
            return player.Hand.FirstOrDefault(card => card.InstanceId == action.SourceId)?.ModelId;
        if (action.Kind != "use_potion" || action.SourceId is null) return null;
        var slot = int.Parse(action.SourceId.Split(':')[^1]);
        return player.Potions.FirstOrDefault(potion => potion.Amount == slot)?.ModelId;
    }

    public void Dispose()
    {
        try { _process.StandardInput.Close(); }
        catch { }
        try { if (!_process.HasExited) _process.Kill(entireProcessTree: true); }
        catch { }
        _process.Dispose();
    }
}

internal static class PolicyAutoSlay
{
    private static PolicyClient? _policy;
    private static TraceCapture? _capture;

    internal static void Install(Harmony harmony, PolicyClient policy, TraceCapture capture)
    {
        _policy = policy;
        _capture = capture;
        harmony.Patch(
            AccessTools.Method(typeof(CombatRoomHandler), nameof(CombatRoomHandler.HandleAsync)),
            prefix: new HarmonyMethod(typeof(PolicyAutoSlay), nameof(BeforeCombat)));
        harmony.Patch(
            AccessTools.Method(typeof(EventRoomHandler), "HandleEventCombat"),
            prefix: new HarmonyMethod(typeof(PolicyAutoSlay), nameof(BeforeEventCombat)));
        harmony.Patch(
            AccessTools.Method(typeof(UiHelper), nameof(UiHelper.Click)),
            prefix: new HarmonyMethod(typeof(PolicyAutoSlay), nameof(BeforeClick)));
        harmony.Patch(
            AccessTools.Method(typeof(NCharacterSelectButton), nameof(NCharacterSelectButton.Select)),
            prefix: new HarmonyMethod(typeof(PolicyAutoSlay), nameof(BeforeCharacterSelect)));
        harmony.Patch(
            AccessTools.Method(typeof(PunchOff), nameof(PunchOff.AfterEventStarted)),
            prefix: new HarmonyMethod(typeof(PolicyAutoSlay), nameof(BeforePunchOff)));
        var nextItem = typeof(Rng).GetMethods().Single(method =>
            method.Name == nameof(Rng.NextItem) && method.IsGenericMethodDefinition);
        var prefix = typeof(PolicyAutoSlay).GetMethod(
            nameof(BeforeRandomChoice), BindingFlags.NonPublic | BindingFlags.Static)!;
        foreach (var type in new[]
                 {
                     typeof(NCharacterSelectButton), typeof(NEventOptionButton),
                     typeof(NRestSiteButton), typeof(NMerchantSlot), typeof(NCardHolder),
                     typeof(CardModel), typeof(NClickableControl)
                 })
            harmony.Patch(nextItem.MakeGenericMethod(type),
                prefix: new HarmonyMethod(prefix.MakeGenericMethod(type)));
    }

    private static bool BeforeCombat(CancellationToken ct, ref Task __result)
    {
        __result = PlayCombat(ct);
        return false;
    }

    private static bool BeforeEventCombat(CancellationToken ct, ref Task __result)
    {
        __result = PlayCombat(ct);
        return false;
    }

    private static bool BeforePunchOff(ref Task __result)
    {
        __result = Task.CompletedTask;
        return false;
    }

    private static async Task PlayCombat(CancellationToken ct)
    {
        await WaitHelper.Until(() => CombatManager.Instance.IsInProgress, ct,
            TimeSpan.FromSeconds(10), "Policy combat did not start");
        var player = RunManager.Instance.DebugOnlyGetState()!.Players.Single();
        while (CombatManager.Instance.IsInProgress)
        {
            await WaitHelper.Until(() => !CombatManager.Instance.IsInProgress ||
                player.PlayerCombatState?.Phase == PlayerTurnPhase.Play,
                ct, TimeSpan.FromSeconds(30), "Policy play phase did not start");
            while (CombatManager.Instance.IsInProgress &&
                   player.PlayerCombatState?.Phase == PlayerTurnPhase.Play)
            {
                var snapshot = _capture!.CaptureForPolicy();
                PolicyChoice choice;
                try { choice = _policy!.Choose(snapshot); }
                catch (Exception exception)
                {
                    Console.Error.WriteLine($"[Spirefysh policy] {exception.Message}; ending turn.");
                    choice = new(0, "end_turn", null, null, null);
                }
                var action = snapshot.Public.Players[0].LegalActions.FirstOrDefault(candidate =>
                    candidate.Kind == choice.Kind &&
                    (string.IsNullOrEmpty(choice.SourceId) || candidate.SourceId == choice.SourceId) &&
                    (string.IsNullOrEmpty(choice.TargetId) || candidate.TargetId == choice.TargetId));
                action ??= snapshot.Public.Players[0].LegalActions.FirstOrDefault(candidate =>
                    candidate.Kind == "play_card" && choice.Kind == "play_card" &&
                    player.PlayerCombatState!.Hand.Cards.Any(card =>
                        SemanticAction.CardIdentity(card) == candidate.SourceId &&
                        card.Id.ToString() == choice.ModelId));
                action ??= snapshot.Public.Players[0].LegalActions.FirstOrDefault(candidate =>
                    candidate.Kind == "use_potion" && choice.Kind == "use_potion" &&
                    player.GetPotionAtSlotIndex(int.Parse(candidate.SourceId!.Split(':')[^1]))
                        ?.Id.ToString() == choice.ModelId);
                action ??= choice.Kind == "end_turn"
                    ? new SemanticAction("end_turn", "end-turn", null, null, nameof(EndPlayerTurnAction))
                    : null;
                if (action is null)
                {
                    Console.Error.WriteLine($"[Spirefysh policy] Unavailable action {choice}; ending turn.");
                    action = new("end_turn", "end-turn", null, null, nameof(EndPlayerTurnAction));
                }
                await Execute(player, action, ct);
                if (action.Kind == "end_turn") break;
                await Task.Delay(25, ct);
            }
        }
    }

    private static async Task Execute(Player player, SemanticAction action, CancellationToken ct)
    {
        GameAction gameAction = action.Kind switch
        {
            "play_card" => new PlayCardAction(
                player.PlayerCombatState!.Hand.Cards.Single(card =>
                    SemanticAction.CardIdentity(card) == action.SourceId),
                CombatManager.Instance.DebugOnlyGetState()!.Creatures.FirstOrDefault(creature =>
                    $"creature:{creature.CombatId}" == action.TargetId)),
            "use_potion" => new UsePotionAction(
                player.GetPotionAtSlotIndex(int.Parse(action.SourceId!.Split(':')[^1]))!,
                CombatManager.Instance.DebugOnlyGetState()!.Creatures.FirstOrDefault(creature =>
                    $"creature:{creature.CombatId}" == action.TargetId),
                isCombatInProgress: true),
            "end_turn" => new EndPlayerTurnAction(player, player.PlayerCombatState!.TurnNumber),
            _ => throw new NotSupportedException($"Policy combat action '{action.Kind}' is unsupported.")
        };
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        gameAction.JustBeforeFinished += _ => completion.TrySetResult();
        gameAction.BeforeCancelled += _ => completion.TrySetResult();
        RunManager.Instance.ActionQueueSet.EnqueueWithoutSynchronizing(gameAction);
        await completion.Task.WaitAsync(TimeSpan.FromSeconds(30), ct);
    }

    private static bool BeforeRandomChoice<T>(IEnumerable<T> items, ref T __result)
    {
        var candidates = items.ToArray();
        if (candidates.Length == 0) return true;
        if (candidates[0] is NCharacterSelectButton)
        {
            var id = System.Environment.GetEnvironmentVariable("SPIREFYSH_CHARACTER_ID");
            __result = candidates.Single(candidate =>
                ((NCharacterSelectButton)(object)candidate!).Character.Id.ToString() == id);
            return false;
        }
        var actions = Actions(candidates.Cast<object>().ToArray());
        var choice = TryChoose(_capture!.CaptureForPolicy(actions));
        if (choice is null) return true;
        var index = ChoiceIndex(candidates.Cast<object>().ToArray(), choice);
        __result = candidates[index];
        return false;
    }

    private static bool BeforeCharacterSelect(NCharacterSelectButton __instance)
    {
        var id = System.Environment.GetEnvironmentVariable("SPIREFYSH_CHARACTER_ID");
        if (__instance.Character.Id.ToString() == id) return true;
        var desired = UiHelper.FindAll<NCharacterSelectButton>(
            ((SceneTree)Engine.GetMainLoop()).Root).Single(button =>
            button.Character.Id.ToString() == id);
        desired.Select();
        return false;
    }

    private static SemanticAction[]? Actions(object[] candidates) => candidates[0] switch
    {
        NEventOptionButton => candidates.Select((candidate, index) => new SemanticAction(
            "event_option", $"event-option:{index}", index.ToString(), null,
            candidate.GetType().FullName!)).ToArray(),
        NRestSiteButton => candidates.Select((candidate, index) => new SemanticAction(
            "rest_site", $"rest:{index}", index.ToString(), null,
            candidate.GetType().FullName!)).ToArray(),
        NMerchantSlot => candidates.Select((candidate, index) => new SemanticAction(
            "shop_purchase", $"shop:{index}", index.ToString(), null,
            candidate.GetType().FullName!)).ToArray(),
        CardModel or NCardHolder => [new(
            "choose_cards", "choose-cards", null, null, "PolicyChoice",
            CandidateIds: candidates.Select(candidate => SemanticAction.CardIdentity(
                candidate is CardModel card ? card : ((NCardHolder)candidate).CardModel!)).ToArray(),
            Minimum: 1, Maximum: 1)],
        _ => null
    };

    private static int ChoiceIndex(object[] candidates, PolicyChoice choice)
    {
        if (candidates[0] is CardModel or NCardHolder)
        {
            var index = Array.FindIndex(candidates, candidate =>
            {
                var card = candidate is CardModel model ? model : ((NCardHolder)candidate).CardModel!;
                return SemanticAction.CardIdentity(card) == choice.SourceId ||
                       card.Id.ToString() == choice.ModelId;
            });
            if (index >= 0) return index;
        }
        if (choice.Index >= 0 && choice.Index < candidates.Length) return choice.Index;
        throw new InvalidOperationException($"Policy choice {choice} does not match {candidates.Length} candidates.");
    }

    private static void BeforeClick(ref NClickableControl button)
    {
        var root = ((SceneTree)Engine.GetMainLoop()).Root;
        if (button is NMapPoint)
        {
            var points = UiHelper.FindAll<NMapPoint>(root).Where(point => point.IsEnabled).ToArray();
            var actions = points.Select(point => new SemanticAction(
                "map_node", $"map:{point.Point.coord.col}:{point.Point.coord.row}", null,
                $"map:{point.Point.coord.col}:{point.Point.coord.row}", point.GetType().FullName!)).ToArray();
            var choice = TryChoose(_capture!.CaptureForPolicy(actions));
            if (choice is null) return;
            button = points.FirstOrDefault(point =>
                $"map:{point.Point.coord.col}:{point.Point.coord.row}" == choice.TargetId) ?? button;
        }
        else if (button is NRewardButton)
        {
            var choice = TryChoose(_capture!.CaptureForPolicy());
            if (choice is null) return;
            if (choice.Kind == "skip_rewards")
                button = UiHelper.FindAll<NProceedButton>(root).FirstOrDefault(candidate => candidate.IsEnabled) ?? button;
            else
            {
                var rewards = UiHelper.FindAll<NRewardButton>(root)
                    .Where(candidate => candidate.IsEnabled).ToArray();
                if (choice.Index < rewards.Length) button = rewards[choice.Index];
            }
        }
        else if (button is NTreasureRoomRelicHolder)
        {
            var holders = UiHelper.FindAll<NTreasureRoomRelicHolder>(root)
                .Where(candidate => candidate.IsEnabled && candidate.Visible).ToArray();
            var choice = TryChoose(_capture!.CaptureForPolicy());
            if (choice is null) return;
            if (choice.Kind == "skip_relic")
                button = UiHelper.FindAll<NProceedButton>(root).First(candidate => candidate.IsEnabled);
            else if (choice.Index < holders.Length)
                button = holders[choice.Index];
        }
    }

    private static PolicyChoice? TryChoose(CapturedSnapshot snapshot)
    {
        try { return _policy!.Choose(snapshot); }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"[Spirefysh policy] {exception.Message}; using AutoSlay fallback.");
            return null;
        }
    }
}
