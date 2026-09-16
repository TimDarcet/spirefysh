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
using MegaCrit.Sts2.Core.Combat;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.Entities.Merchant;
using MegaCrit.Sts2.Core.Nodes.Cards.Holders;
using MegaCrit.Sts2.Core.Nodes.Combat;
using MegaCrit.Sts2.Core.Nodes.Events;
using MegaCrit.Sts2.Core.Nodes.Potions;
using MegaCrit.Sts2.Core.Nodes.RestSite;
using MegaCrit.Sts2.Core.Nodes.Screens.Map;
using MegaCrit.Sts2.Core.Nodes.Screens.Shops;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Runs;

namespace Spirefysh.Bridge;

internal sealed record AdvisorConfiguration(bool ShowWinProbabilityDelta, bool ShowCurrentWinProbability, string Python, string Model);
internal sealed record AdvisorSelector(string? Kind = null, string? SourceId = null, string? TargetId = null, string? ModelId = null);
internal sealed record AdvisorResult(double WinProbability, double Delta, string? Error = null);

internal sealed class AdvisorClient : IDisposable
{
    private const int CacheCapacity = 256;
    private readonly Process _process;
    private readonly object _gate = new();
    private readonly Dictionary<string, AdvisorResult> _cache = [];
    private readonly Queue<string> _cacheOrder = [];
    private readonly CancellationTokenSource _stopping = new();
    private Work? _queued;
    private Task? _worker;
    private string? _activeKey;
    private Exception? _failure;
    private bool _disposed;

    private sealed record Work(
        string Key,
        string Request,
        int Priority);

    internal bool ShowWinProbabilityDelta { get; }
    internal bool ShowCurrentWinProbability { get; }

    private AdvisorClient(Process process, AdvisorConfiguration configuration)
    {
        _process = process;
        ShowWinProbabilityDelta = configuration.ShowWinProbabilityDelta;
        ShowCurrentWinProbability = configuration.ShowCurrentWinProbability;
    }

    internal static AdvisorClient? CreateFromConfiguration()
    {
        var assembly = typeof(AdvisorClient).Assembly.Location;
        var directory = Path.GetDirectoryName(assembly)!;
        var path = Path.Combine(directory, "Spirefysh.Bridge.advisor-config");
        if (!File.Exists(path)) return null;
        var config = JsonSerializer.Deserialize<AdvisorConfiguration>(File.ReadAllText(path), TraceJson.Options)
            ?? throw new InvalidDataException("Invalid advisor configuration.");
        if (!config.ShowWinProbabilityDelta && !config.ShowCurrentWinProbability) return null;
        var python = Resolve(directory, config.Python);
        var model = Resolve(directory, config.Model);
        var script = Path.Combine(directory, "advisor.py");
        foreach (var required in new[] { python, model, script })
            if (!File.Exists(required)) throw new FileNotFoundException("Advisor dependency is missing.", required);
        var start = new ProcessStartInfo(python)
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true
        };
        start.ArgumentList.Add("-u");
        start.ArgumentList.Add(script);
        start.ArgumentList.Add("serve");
        start.ArgumentList.Add(model);
        var process = Process.Start(start) ?? throw new InvalidOperationException("Could not start the advisor process.");
        process.ErrorDataReceived += (_, eventArgs) =>
        {
            if (!string.IsNullOrWhiteSpace(eventArgs.Data)) Console.Error.WriteLine($"[Spirefysh advisor] {eventArgs.Data}");
        };
        process.BeginErrorReadLine();
        return new(process, config);
    }

    internal AdvisorResult? Analyze(SemanticAction action, AdvisorSelector selector) =>
        Analyze(Snapshotter.Capture([action]), selector, 1);

    internal AdvisorResult? AnalyzeCurrent() => Analyze(Snapshotter.Capture(), new(), 0);

    private AdvisorResult? Analyze(CapturedSnapshot snapshot, AdvisorSelector selector, int priority)
    {
        var key = $"{snapshot.PublicHash}:{snapshot.OracleHash}:{selector}";
        var request = JsonSerializer.Serialize(new
        {
            before = snapshot.Public,
            oracle_before = snapshot.Oracle,
            selector
        }, TraceJson.Options);
        return Enqueue(key, request, priority);
    }

    private AdvisorResult? Enqueue(string key, string request, int priority)
    {
        lock (_gate)
        {
            if (_cache.TryGetValue(key, out var cached)) return cached;
            ObjectDisposedException.ThrowIf(_disposed, this);
            if (_failure is not null)
                throw new InvalidOperationException("Advisor process is unavailable.", _failure);
            if (key == _activeKey || key == _queued?.Key) return null;
            var work = new Work(key, request, priority);
            if (_queued is null || priority >= _queued.Priority) _queued = work;
            _worker ??= Task.Run(ProcessQueue);
            return null;
        }
    }

    private void ProcessQueue()
    {
        while (true)
        {
            Work? work;
            lock (_gate)
            {
                if (_disposed || _queued is null)
                {
                    _activeKey = null;
                    _worker = null;
                    return;
                }
                work = _queued;
                _queued = null;
                _activeKey = work.Key;
            }
            AdvisorResult result;
            try
            {
                result = Request(work.Request);
            }
            catch (Exception exception)
            {
                lock (_gate)
                {
                    if (!_disposed) _failure = exception;
                    _activeKey = null;
                    _worker = null;
                }
                return;
            }
            lock (_gate)
            {
                if (_disposed) return;
                if (_cache.Count == CacheCapacity) _cache.Remove(_cacheOrder.Dequeue());
                _cache[work.Key] = result;
                _cacheOrder.Enqueue(work.Key);
                _activeKey = null;
            }
        }
    }

    private AdvisorResult Request(string request)
    {
        try
        {
            if (_process.HasExited) throw new InvalidOperationException("Advisor process exited.");
            _process.StandardInput.WriteLine(request);
            _process.StandardInput.Flush();
            var response = _process.StandardOutput.ReadLineAsync(_stopping.Token).AsTask()
                .WaitAsync(TimeSpan.FromSeconds(5), _stopping.Token).GetAwaiter().GetResult()
                ?? throw new EndOfStreamException("Advisor process closed its output.");
            var result = JsonSerializer.Deserialize<AdvisorResult>(response, TraceJson.Options)
                ?? throw new InvalidDataException("Advisor returned an empty response.");
            if (result.Error is not null) throw new InvalidOperationException(result.Error);
            return result;
        }
        catch
        {
            try { if (!_process.HasExited) _process.Kill(entireProcessTree: true); }
            catch { }
            throw;
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_disposed) return;
            _disposed = true;
            _queued = null;
            _stopping.Cancel();
        }
        try { _process.StandardInput.Close(); }
        catch { }
        try { if (!_process.HasExited) _process.Kill(entireProcessTree: true); }
        catch { }
        _process.Dispose();
        _stopping.Dispose();
    }

    private static string Resolve(string directory, string path) =>
        Path.GetFullPath(Path.IsPathFullyQualified(path) ? path : Path.Combine(directory, path));
}

internal static class AdvisorTopBar
{
    private static AdvisorClient? _client;
    private static SceneTree? _tree;
    private static Label? _label;
    private static long _nextUpdate;

    internal static void Install(AdvisorClient client)
    {
        _client = client;
        _tree = Engine.GetMainLoop() as SceneTree
            ?? throw new InvalidOperationException("Godot scene tree is unavailable.");
        _tree.ProcessFrame += Update;
    }

    private static void Update()
    {
        var now = System.Environment.TickCount64;
        if (now < _nextUpdate) return;
        _nextUpdate = now + 500;
        try
        {
            var result = _client?.AnalyzeCurrent();
            if (result is null) return;
            Label().Text = $"Win {result.WinProbability:P1}";
        }
        catch
        {
            if (RunManager.Instance.DebugOnlyGetState() is not { IsGameOver: false } && _label is not null)
                _label.Visible = false;
        }
    }

    private static Label Label()
    {
        if (_label is not null)
        {
            _label.Visible = true;
            return _label;
        }
        _label = new Label
        {
            AnchorLeft = 0.5f,
            AnchorRight = 0.5f,
            OffsetLeft = -120,
            OffsetRight = 120,
            OffsetTop = 8,
            OffsetBottom = 48,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            MouseFilter = Control.MouseFilterEnum.Ignore,
            ZIndex = 4096
        };
        _label.AddThemeFontSizeOverride("font_size", 24);
        _label.AddThemeColorOverride("font_color", Colors.White);
        _label.AddThemeColorOverride("font_outline_color", Colors.Black);
        _label.AddThemeConstantOverride("outline_size", 8);
        _tree!.Root.AddChild(_label);
        return _label;
    }
}

internal static class AdvisorHover
{
    private static AdvisorClient? _client;
    private static SceneTree? _tree;
    private static Label? _label;
    private static Control? _owner;
    private static SemanticAction? _action;
    private static AdvisorSelector? _selector;
    private static long _nextUpdate;

    internal static void Install(Harmony harmony, AdvisorClient client)
    {
        _client = client;
        _tree = Engine.GetMainLoop() as SceneTree
            ?? throw new InvalidOperationException("Godot scene tree is unavailable.");
        _tree.ProcessFrame += Update;
        foreach (var type in new[]
                 {
                     typeof(NCardHolder), typeof(NHandCardHolder), typeof(NEndTurnButton),
                     typeof(NPotionHolder), typeof(NMapPoint), typeof(NRestSiteButton),
                     typeof(NEventOptionButton), typeof(NMerchantSlot)
                 })
        {
            Patch(harmony, type, "OnUnfocus", nameof(Hide));
        }
        Patch(harmony, typeof(NCardHolder), "OnFocus", nameof(CardFocused));
        Patch(harmony, typeof(NHandCardHolder), "OnFocus", nameof(CardFocused));
        Patch(harmony, typeof(NEndTurnButton), "OnFocus", nameof(EndTurnFocused));
        Patch(harmony, typeof(NPotionHolder), "OnFocus", nameof(PotionFocused));
        Patch(harmony, typeof(NMapPoint), "OnFocus", nameof(MapFocused));
        Patch(harmony, typeof(NRestSiteButton), "OnFocus", nameof(RestFocused));
        Patch(harmony, typeof(NEventOptionButton), "OnFocus", nameof(EventFocused));
        Patch(harmony, typeof(NMerchantSlot), "OnFocus", nameof(MerchantFocused));
    }

    private static void Patch(Harmony harmony, Type type, string target, string patch) =>
        harmony.Patch(
            AccessTools.Method(type, target) ?? throw new MissingMethodException(type.FullName, target),
            postfix: new HarmonyMethod(typeof(AdvisorHover), patch));

    private static void CardFocused(NCardHolder __instance)
    {
        var card = __instance.CardModel;
        if (card is null) return;
        if (CombatManager.Instance.IsInProgress)
        {
            var source = SemanticAction.CardIdentity(card);
            Show(__instance, new("play_card", $"play:{source}", source, null, nameof(CardModel)),
                new(SourceId: source));
        }
        else
            Show(__instance, new("choose_cards", "choose-card", null, null, nameof(CardModel)),
                new(ModelId: card.Id.ToString()));
    }

    private static void EndTurnFocused(NEndTurnButton __instance) =>
        Show(__instance, new("end_turn", "end-turn", null, null, nameof(NEndTurnButton)), new(Kind: "end_turn"));

    private static void PotionFocused(NPotionHolder __instance)
    {
        var potion = __instance.Potion?.Model;
        var player = RunManager.Instance.DebugOnlyGetState()?.Players.SingleOrDefault();
        if (potion is null || player is null) return;
        var source = $"potion-slot:{player.GetPotionSlotIndex(potion)}";
        Show(__instance, new("use_potion", $"potion:{source}", source, null, nameof(NPotionHolder)),
            new(Kind: "use_potion", SourceId: source));
    }

    private static void MapFocused(NMapPoint __instance)
    {
        var target = $"map:{__instance.Point.coord.col}:{__instance.Point.coord.row}";
        Show(__instance, new("map_node", target, null, target, nameof(NMapPoint)),
            new(Kind: "map_node", TargetId: target));
    }

    private static void RestFocused(NRestSiteButton __instance)
    {
        var source = __instance.Option.OptionId.ToString();
        Show(__instance, new("rest_site", $"rest:{source}", source, null, nameof(NRestSiteButton)),
            new(Kind: "rest_site", SourceId: source));
    }

    private static void EventFocused(NEventOptionButton __instance)
    {
        var index = (int)(AccessTools.Field(typeof(NEventOptionButton), "<Index>k__BackingField").GetValue(__instance) ?? -1);
        var id = $"event:{__instance.Event.Id}:{index}:{__instance.Option.TextKey}";
        Show(__instance, new("event_option", id, $"event:{__instance.Event.Id}", null, nameof(NEventOptionButton)),
            new(Kind: "event_option", SourceId: index.ToString()));
    }

    private static void MerchantFocused(NMerchantSlot __instance)
    {
        var model = __instance.Entry switch
        {
            MerchantCardEntry card => card.CreationResult?.Card?.Id.ToString(),
            MerchantRelicEntry relic => relic.Model?.Id.ToString(),
            MerchantPotionEntry potion => potion.Model?.Id.ToString(),
            MerchantCardRemovalEntry => "card-removal",
            _ => null
        };
        if (model is null) return;
        Show(__instance, new("shop_purchase", $"shop:{model}:{__instance.Entry.Cost}", model, null, __instance.Entry.GetType().Name),
            new(Kind: "shop_purchase", ModelId: model));
    }

    private static void Show(Control owner, SemanticAction action, AdvisorSelector selector)
    {
        _owner = owner;
        _action = action;
        _selector = selector;
        _nextUpdate = 0;
        Update();
    }

    private static void Update()
    {
        if (_owner is not { } owner || _action is not { } action || _selector is not { } selector)
            return;
        var now = System.Environment.TickCount64;
        if (now < _nextUpdate) return;
        _nextUpdate = now + 100;
        try
        {
            var result = _client?.Analyze(action, selector);
            if (result is null) return;
            var delta = result.Delta * 100;
            Label(owner).Text = $"Win {result.WinProbability:P1}   Δ {delta:+0.0;-0.0;0.0} pp vs best";
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine($"[Spirefysh advisor] {exception.Message}");
            Hide();
        }
    }

    private static Label Label(Control owner)
    {
        if (_label is null)
        {
            _label = new Label { MouseFilter = Control.MouseFilterEnum.Ignore, ZIndex = 4096 };
            _label.AddThemeFontSizeOverride("font_size", 24);
            _label.AddThemeColorOverride("font_color", Colors.White);
            _label.AddThemeColorOverride("font_outline_color", Colors.Black);
            _label.AddThemeConstantOverride("outline_size", 8);
            owner.GetTree().Root.AddChild(_label);
        }
        _label.Position = owner.GetViewport().GetMousePosition() + new Vector2(18, 18);
        _label.Visible = true;
        return _label;
    }

    private static void Hide()
    {
        _owner = null;
        _action = null;
        _selector = null;
        if (_label is not null) _label.Visible = false;
    }
}
