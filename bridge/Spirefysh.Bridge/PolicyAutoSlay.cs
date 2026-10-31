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
using MegaCrit.Sts2.Core.AutoSlay;
using MegaCrit.Sts2.Core.AutoSlay.Handlers;
using MegaCrit.Sts2.Core.AutoSlay.Handlers.Rooms;
using MegaCrit.Sts2.Core.AutoSlay.Handlers.Screens;
using MegaCrit.Sts2.Core.AutoSlay.Helpers;
using MegaCrit.Sts2.Core.Combat;
using MegaCrit.Sts2.Core.Commands;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.Entities.Merchant;
using MegaCrit.Sts2.Core.Entities.Players;
using MegaCrit.Sts2.Core.GameActions;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Models.Events;
using MegaCrit.Sts2.Core.Nodes;
using MegaCrit.Sts2.Core.Nodes.Audio;
using MegaCrit.Sts2.Core.Nodes.Cards.Holders;
using MegaCrit.Sts2.Core.Nodes.CommonUi;
using MegaCrit.Sts2.Core.Nodes.Events;
using MegaCrit.Sts2.Core.Nodes.GodotExtensions;
using MegaCrit.Sts2.Core.Nodes.RestSite;
using MegaCrit.Sts2.Core.Nodes.Rooms;
using MegaCrit.Sts2.Core.Nodes.Rewards;
using MegaCrit.Sts2.Core.Nodes.Screens.CharacterSelect;
using MegaCrit.Sts2.Core.Nodes.Screens.Map;
using MegaCrit.Sts2.Core.Nodes.Screens.Overlays;
using MegaCrit.Sts2.Core.Nodes.Screens.Shops;
using MegaCrit.Sts2.Core.Nodes.Screens.TreasureRoomRelic;
using MegaCrit.Sts2.Core.Random;
using MegaCrit.Sts2.Core.Runs;
using MegaCrit.Sts2.Core.Saves.Managers;

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
    private static System.Threading.Timer? _stallGuard;

    internal static void Install(Harmony harmony, PolicyClient policy, TraceCapture capture)
    {
        _policy = policy;
        _capture = capture;
        Engine.TimeScale = 10;
        HarmonyMethod Hook(string name) => new(typeof(PolicyAutoSlay), name);
        harmony.Patch(AccessTools.Method(typeof(RunManager), nameof(RunManager.OnEnded)), postfix: Hook(nameof(AfterRunEnded)));
        harmony.Patch(AccessTools.Method(typeof(ProgressSaveManager), nameof(ProgressSaveManager.SaveProgress)), prefix: Hook(nameof(SkipProgressSave)));
        harmony.Patch(AccessTools.Method(typeof(CombatRoomHandler), nameof(CombatRoomHandler.HandleAsync)), prefix: Hook(nameof(BeforeCombat)));
        harmony.Patch(AccessTools.Method(typeof(EventRoomHandler), "HandleEventCombat"), prefix: Hook(nameof(BeforeEventCombat)));
        harmony.Patch(AccessTools.Method(typeof(ShopRoomHandler), nameof(ShopRoomHandler.HandleAsync)), prefix: Hook(nameof(BeforeShop)));
        // Wall-clock room limits sized for plain AutoSlay kill deep relic runs on a loaded machine
        // (shops buying everything, long boss fights); StallGuard catches hangs within minutes.
        foreach (var handler in new[] { typeof(CombatRoomHandler), typeof(EventRoomHandler), typeof(ShopRoomHandler) })
            harmony.Patch(AccessTools.PropertyGetter(handler, nameof(CombatRoomHandler.Timeout)), postfix: Hook(nameof(LongRoomTimeout)));
        harmony.Patch(AccessTools.Method(typeof(UiHelper), nameof(UiHelper.Click)), prefix: Hook(nameof(BeforeClick)));
        harmony.Patch(AccessTools.Method(typeof(NCharacterSelectButton), nameof(NCharacterSelectButton.Select)), prefix: Hook(nameof(BeforeCharacterSelect)));
        harmony.Patch(AccessTools.Method(typeof(PunchOff), nameof(PunchOff.AfterEventStarted)), prefix: Hook(nameof(BeforePunchOff)));
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
        harmony.Patch(AccessTools.Method(typeof(NAudioManager), nameof(NAudioManager.SetMasterVol)), prefix: Hook(nameof(Mute)));
        harmony.Patch(AccessTools.Method(typeof(RunManager), nameof(RunManager.FinalizeStartingRelics)), prefix: Hook(nameof(AddStartingLoadout)));
        harmony.Patch(AccessTools.AsyncMoveNext(AccessTools.Method(typeof(AutoSlayer), "PlayRunAsync")), transpiler: Hook(nameof(RaiseFloorLimit)));
        // A frozen game (or an await nobody completes) stops resetting AutoSlay's watchdog. Polled from a
        // timer thread on the monotonic clock, so it survives a frozen main thread and ignores Mac sleep.
        DateTime seenProgress = default;
        var seenAt = Stopwatch.GetTimestamp();
        _stallGuard = new System.Threading.Timer(_ =>
        {
            if (!AutoSlayer.IsActive || AutoSlayer.CurrentWatchdog is not { } watchdog) return;
            var progress = (DateTime)AccessTools.Field(typeof(Watchdog), "_lastProgressTime").GetValue(watchdog)!;
            if (progress != seenProgress) (seenProgress, seenAt) = (progress, Stopwatch.GetTimestamp());
            if (Stopwatch.GetElapsedTime(seenAt) < TimeSpan.FromMinutes(3)) return;
            Console.Error.WriteLine("[Spirefysh] No AutoSlay progress for 3 minutes; killing the game.");
            Process.GetCurrentProcess().Kill(entireProcessTree: true);
        }, null, TimeSpan.FromSeconds(10), TimeSpan.FromSeconds(10));
    }

    // AutoSlay stops at floor 49 and tries to abandon, but the Ascension 10 double boss is on floor 50.
    private static IEnumerable<CodeInstruction> RaiseFloorLimit(IEnumerable<CodeInstruction> instructions) =>
        instructions.Select(instruction => instruction.LoadsConstant(49)
            ? new CodeInstruction(System.Reflection.Emit.OpCodes.Ldc_I4, 1000).MoveLabelsFrom(instruction)
            : instruction);

    // SPIREFYSH_START_RELICS (comma-separated relic ids) joins the starting relics, so their
    // AfterObtained effects fire like the starter relic's. SPIREFYSH_START_CARDS adds deck cards,
    // optionally enchanted: CARD.REANIMATE@ENCHANTMENT.IMBUED.
    private static void AddStartingLoadout()
    {
        var state = RunManager.Instance.DebugOnlyGetState()!;
        static string[] List(string name) => (System.Environment.GetEnvironmentVariable(name) ?? "")
            .Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        foreach (var player in state.Players)
            foreach (var entry in List("SPIREFYSH_START_CARDS"))
            {
                var parts = entry.Split('@');
                var card = state.CreateCard(ModelDb.AllCards.Single(card => card.Id.ToString() == parts[0]), player);
                card.FloorAddedToDeck = 1;
                if (parts.Length > 1)
                    CardCmd.Enchant(ModelDb.DebugEnchantments.Single(enchantment =>
                        enchantment.Id.ToString() == parts[1]).ToMutable(), card, 1m);
                player.Deck.AddInternal(card);
            }
        foreach (var player in state.Players)
            foreach (var id in List("SPIREFYSH_START_RELICS"))
            {
                var relic = ModelDb.AllRelics.Single(relic => relic.Id.ToString() == id).ToMutable();
                relic.FloorAddedToDeck = 1;
                player.AddRelicInternal(relic);
                player.RelicGrabBag.Remove(relic);
                state.SharedRelicGrabBag.Remove(relic);
            }
    }

    private static void Mute(ref float volume) => volume = 0;

    // AutoSlay would otherwise wait 10s for a rewards screen after a death.
    private static void AfterRunEnded() => NGame.Instance?.GetTree().Quit();

    // Parallel samplers would race on progress.save; it also keeps unlocks fixed across samples.
    private static bool SkipProgressSave() => false;

    private static bool BeforeCombat(CancellationToken ct, ref Task __result)
    {
        __result = PlayCombat(ct);
        return false;
    }

    private static void LongRoomTimeout(ref TimeSpan __result) => __result = TimeSpan.FromMinutes(25);

    private static bool BeforeShop(Rng random, CancellationToken ct, ref Task __result)
    {
        __result = PlayShop(random, ct);
        return false;
    }

    // ShopRoomHandler, except the screens a purchase opens get handled while it waits: relics like
    // Orrery and Cauldron offer rewards on pickup, and the purchase only completes once they're claimed.
    private static async Task PlayShop(Rng random, CancellationToken ct)
    {
        AutoSlayLog.Action("Waiting for shop room");
        var room = await WaitHelper.ForNode<NMerchantRoom>(((SceneTree)Engine.GetMainLoop()).Root,
            "/root/Game/RootSceneContainer/Run/RoomContainer/MerchantRoom", ct);
        AutoSlayLog.Action("Opening merchant inventory");
        room.OpenInventory();
        await Task.Delay(500, ct);
        for (var attempt = 0; attempt < 50; attempt++)
        {
            ct.ThrowIfCancellationRequested();
            var slots = room.Inventory.GetAllSlots()
                .Where(slot => slot is not NMerchantCardRemoval && slot.Entry.IsStocked && slot.Entry.EnoughGold)
                .ToList();
            if (slots.Count == 0)
            {
                AutoSlayLog.Action("No more affordable items to buy");
                break;
            }
            var slot = random.NextItem(slots)!;
            AutoSlayLog.Action($"Buying item (cost: {slot.Entry.Cost})");
            var purchase = slot.Entry.OnTryPurchaseWrapper(room.Inventory.Inventory);
            while (!purchase.IsCompleted)
            {
                await DrainScreens(random, ct);
                await Task.Delay(100, ct);
            }
            await purchase;
            AutoSlayer.CurrentWatchdog?.Reset("Shop: bought an item");
            await Task.Delay(300, ct);
        }
        if (UiHelper.FindFirst<NBackButton>(room) is { } back)
        {
            AutoSlayLog.Action("Closing inventory");
            await UiHelper.Click(back);
            await Task.Delay(300, ct);
        }
        AutoSlayLog.Action("Clicking proceed");
        await UiHelper.Click(room.ProceedButton);
    }

    private static readonly Dictionary<Type, IScreenHandler> ScreenHandlers = new IScreenHandler[]
    {
        new RewardsScreenHandler(), new CardRewardScreenHandler(), new DeckUpgradeScreenHandler(),
        new DeckTransformScreenHandler(), new DeckEnchantScreenHandler(), new DeckCardSelectScreenHandler(),
        new SimpleCardSelectScreenHandler(), new ChooseACardScreenHandler(), new ChooseABundleScreenHandler(),
        new ChooseARelicScreenHandler()
    }.ToDictionary(handler => handler.ScreenType);

    // AutoSlayer.DrainOverlayScreensAsync, which only runs between rooms.
    private static async Task DrainScreens(Rng random, CancellationToken ct)
    {
        var handled = new HashSet<IOverlayScreen>();
        var repeats = 0;
        while (NOverlayStack.Instance?.Peek() is { } screen)
        {
            ct.ThrowIfCancellationRequested();
            if (handled.Add(screen)) repeats = 0;
            else if (++repeats >= 3)
                throw new InvalidOperationException($"Screen {screen.GetType().Name} not closing after being handled");
            if (!ScreenHandlers.TryGetValue(screen.GetType(), out var handler))
            {
                AutoSlayLog.Warn($"No handler for screen type: {screen.GetType().Name}");
                return;
            }
            AutoSlayer.CurrentWatchdog?.Reset($"Shop: handling screen {screen.GetType().Name}");
            AutoSlayLog.Info($"Handling screen: {screen.GetType().Name}");
            await WaitHelper.WithTimeout(token => handler.HandleAsync(random, token), handler.Timeout, ct);
            await Task.Delay(100, ct);
        }
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
            var waitStart = DateTime.UtcNow;
            while (CombatManager.Instance.IsInProgress && player.PlayerCombatState?.Phase != PlayerTurnPhase.Play)
            {
                if (DateTime.UtcNow - waitStart > TimeSpan.FromMinutes(2))
                    throw new AutoSlayTimeoutException("Policy play phase did not start");
                AutoSlayer.CurrentWatchdog?.Reset("Policy combat: waiting for the play phase");
                await Task.Delay(100, ct);
            }
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
                // The policy loop replaces CombatRoomHandler, which is what keeps AutoSlay's watchdog alive.
                AutoSlayer.CurrentWatchdog?.Reset($"Policy combat: {action.Kind}");
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
        // Reward buttons keep AutoSlay's click: live snapshots carry no reward contents.
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
