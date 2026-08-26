using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using MegaCrit.Sts2.Core.Combat;
using MegaCrit.Sts2.Core.Combat.History.Entries;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.Entities.Creatures;
using MegaCrit.Sts2.Core.Entities.Multiplayer;
using MegaCrit.Sts2.Core.Entities.Players;
using MegaCrit.Sts2.Core.GameActions;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;
using MegaCrit.Sts2.Core.Map;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.MonsterMoves.Intents;
using MegaCrit.Sts2.Core.Random;
using MegaCrit.Sts2.Core.Runs;

namespace Spirefysh.Bridge;

internal sealed class TraceCapture : IDisposable
{
    private readonly object _writeLock = new();
    private readonly object _decisionLock = new();
    private readonly AppendOnlyTraceWriter _writer;
    private readonly string _sessionId = Guid.NewGuid().ToString("N");
    private readonly string _episodeId = Guid.NewGuid().ToString("N");
    private readonly string _bridgeSha256;
    private readonly List<PendingDecision> _openDecisions = [];
    private ChoiceOffer? _choiceOffer;
    private MegaCrit.Sts2.Core.Rewards.RewardsSet? _rewardsSet;
    private int _decisionIndex;
    private int _nextGapId = 1;
    private int? _activeGapId;
    private int _gapFaultIndex;
    private bool _completed;

    internal const uint TraceSchemaVersion = 2;
    internal const string ContentManifestSha256 =
        "9d6e30675e62e87886375d6f1f701e5b0fb8f0ccd5bef0268eed7e5399a9e604";

    private TraceCapture(AppendOnlyTraceWriter writer, string bridgeSha256)
    {
        _writer = writer;
        _bridgeSha256 = bridgeSha256;
    }
    public bool Enabled { get; private set; } = true;
    public bool HasChoiceOffer => _choiceOffer is not null;
    public string AutoSlaySeed => _episodeId[..12];

    public static TraceCapture? CreateFromConfiguration()
    {
        var configuredPath = Environment.GetEnvironmentVariable("SPIREFYSH_TRACE_PATH");
        if (string.IsNullOrWhiteSpace(configuredPath))
        {
            var configurationPath = Path.ChangeExtension(typeof(TraceCapture).Assembly.Location, ".trace-path");
            if (File.Exists(configurationPath)) configuredPath = File.ReadAllText(configurationPath).Trim();
        }
        if (string.IsNullOrWhiteSpace(configuredPath)) return null;
        if (!Path.IsPathFullyQualified(configuredPath)) throw new InvalidOperationException("SPIREFYSH_TRACE_PATH must be absolute.");
        configuredPath = Path.GetFullPath(configuredPath);
        var extension = Path.GetExtension(configuredPath);
        if (!extension.Equals(".ndjson", StringComparison.OrdinalIgnoreCase)) throw new InvalidOperationException("SPIREFYSH_TRACE_PATH must end in .ndjson.");
        var parent = Path.GetDirectoryName(configuredPath) ?? throw new InvalidOperationException("Trace path has no parent directory.");
        if (!Directory.Exists(parent)) throw new DirectoryNotFoundException($"Trace parent directory does not exist: {parent}");
        RefuseSymlinkPath(parent);
        var markerPath = Path.Combine(parent, ".spirefysh-oracle-output");
        if (!File.Exists(markerPath) || File.ReadAllText(markerPath) != "spirefysh-oracle-output-v1\n")
            throw new InvalidOperationException($"Trace parent must contain the exact oracle-output marker: {markerPath}");
        var bridgeDirectory = Path.GetDirectoryName(typeof(TraceCapture).Assembly.Location)
            ?? throw new InvalidOperationException("Bridge assembly directory is unavailable.");
        var gameContentsDirectory = FindAncestor(bridgeDirectory, "Contents")
            ?? throw new InvalidOperationException("Bridge must be loaded from the pinned macOS app installation.");
        if (IsWithin(configuredPath, gameContentsDirectory))
            throw new InvalidOperationException("Trace output must be outside the game installation.");
        var bridgeAssemblyPath = typeof(TraceCapture).Assembly.Location;
        using var bridgeStream = File.OpenRead(bridgeAssemblyPath);
        var bridgeSha256 = Convert.ToHexString(SHA256.HashData(bridgeStream)).ToLowerInvariant();
        var capture = new TraceCapture(AppendOnlyTraceWriter.Open(configuredPath), bridgeSha256);
        capture.Write(new TraceEvent(0, "episode_started", null, null, null, null, null, null,
            new Dictionary<string, object?>
            {
                ["complete"] = false,
                ["resumed_append"] = capture._writer.LastSequence > 0
            }));
        Console.WriteLine($"[Spirefysh] Trace opened: {configuredPath}");
        return capture;
    }

    private static void RefuseSymlinkPath(string path)
    {
        var current = new DirectoryInfo(Path.GetFullPath(path));
        while (current is not null)
        {
            if ((current.Attributes & FileAttributes.ReparsePoint) != 0)
                throw new InvalidOperationException($"Trace path may not traverse a symbolic link: {current.FullName}");
            current = current.Parent;
        }
    }

    private static bool IsWithin(string candidate, string directory)
    {
        var relative = Path.GetRelativePath(Path.GetFullPath(directory), Path.GetFullPath(candidate));
        return relative != ".." && !relative.StartsWith($"..{Path.DirectorySeparatorChar}", StringComparison.Ordinal)
            && !Path.IsPathFullyQualified(relative);
    }

    private static string? FindAncestor(string path, string name)
    {
        var current = new DirectoryInfo(Path.GetFullPath(path));
        while (current is not null)
        {
            if (current.Name.Equals(name, StringComparison.Ordinal)) return current.FullName;
            current = current.Parent;
        }
        return null;
    }

    public void OnRunStarted(RunState run)
    {
        Guard("run start", () =>
        {
            if (RunManager.Instance.ShouldSave)
            {
                Disable("RunManager.ShouldSave was true");
                return;
            }
            var character = run.Players.SingleOrDefault()?.Character.Id.ToString();
            if (run.GameMode != GameMode.Standard || run.Players.Count != 1 ||
                character is not ("CHARACTER.IRONCLAD" or "CHARACTER.SILENT" or "CHARACTER.DEFECT" or
                    "CHARACTER.REGENT" or "CHARACTER.NECROBINDER") || run.AscensionLevel is < 0 or > 10)
            {
                CaptureGap("run scope",
                    $"mode={run.GameMode} ascension={run.AscensionLevel} players={run.Players.Count} character={character}");
                return;
            }
            WriteSnapshotEvent("run_started");
            Console.WriteLine($"[Spirefysh] RECORDING episode={_episodeId} schema={TraceSchemaVersion}");
        });
    }

    public void OnCombatSetUp(CombatState state) => Guard("combat setup", () => RecordLifecycle("combat_setup"));
    public void OnCombatEnded(MegaCrit.Sts2.Core.Rooms.CombatRoom room) => Guard("combat end", () =>
    {
        var snapshot = Snapshotter.Capture(room.CombatState);
        SettleOpenDecision("combat_ended", snapshot);
        WriteSnapshotEvent("combat_ended", snapshot);
    });

    public void OnActionEnqueued(GameAction action) => Guard("action enqueue", () =>
    {
        if (!MegaCrit.Sts2.Core.GameActions.Multiplayer.ActionQueueSet.IsGameActionPlayerDriven(action)) return;
        if (action is GenericHookGameAction or VoteForMapCoordAction or VoteToMoveToNextActAction) return;
        var semanticAction = SemanticAction.From(action);
        if (semanticAction.Kind == "unknown")
        {
            var snapshot = Snapshotter.Capture();
            Write(new TraceEvent(0, "unsupported_action", action.Id, snapshot.Public, semanticAction, null, snapshot.Oracle, null, null)
            {
                BeforePublicHash = snapshot.PublicHash,
                BeforeOracleHash = snapshot.OracleHash
            });
            CaptureGap("unsupported player action", action.GetType().FullName ?? action.GetType().Name);
            return;
        }
        action.BeforeExecuted += BeforeAction;
        action.BeforePausedForPlayerChoice += BeforeChoice;
        action.BeforeResumedAfterPlayerChoice += BeforeResume;
        action.JustBeforeFinished += AfterAction;
        action.BeforeCancelled += CancelAction;
    });

    private void BeforeAction(GameAction action) => Guard("action start", () =>
    {
        var semanticAction = SemanticAction.From(action);
        var snapshot = Snapshotter.Capture(SemanticAction.LegalActionsFor(action), semanticAction);
        Resynchronize(snapshot, "player_action");
        SettleOpenDecisions("next_player_action", snapshot, nestedOnly: false);
        lock (_decisionLock)
            _openDecisions.Add(new(
                action,
                action.Id,
                semanticAction,
                snapshot.Public,
                snapshot.Oracle,
                snapshot.PublicHash,
                snapshot.OracleHash,
                IsNested: false));
    });

    private void BeforeChoice(GameAction action) => WriteBoundary("choice_requested", action);
    private void BeforeResume(GameAction action) => WriteBoundary("choice_resumed", action);

    private void WriteBoundary(string kind, GameAction action) => Guard(kind, () =>
    {
        var snapshot = Snapshotter.Capture();
        Write(new TraceEvent(0, kind, action.Id, snapshot.Public, SemanticAction.From(action), null, snapshot.Oracle, null, null)
        {
            BeforePublicHash = snapshot.PublicHash,
            BeforeOracleHash = snapshot.OracleHash
        });
    });

    private void AfterAction(GameAction action) => Guard("action completion", () =>
    {
        PendingDecision? pending;
        lock (_decisionLock) pending = _openDecisions.LastOrDefault(candidate =>
            ReferenceEquals(candidate.RuntimeAction, action));
        if (pending is null)
            throw new InvalidOperationException($"Completed player action was not open: {action.Id}");
        var after = Snapshotter.Capture();
        Write(new TraceEvent(0, "action_completed", action.Id, pending.Before, pending.Action,
            after.Public, pending.BeforeOracle, after.Oracle,
            new Dictionary<string, object?> { ["settled"] = false })
        {
            BeforePublicHash = pending.BeforePublicHash,
            BeforeOracleHash = pending.BeforeOracleHash,
            AfterPublicHash = after.PublicHash,
            AfterOracleHash = after.OracleHash
        });
    });

    private void CancelAction(GameAction action) => Guard("action cancellation", () =>
    {
        lock (_decisionLock)
            _openDecisions.RemoveAll(candidate => ReferenceEquals(candidate.RuntimeAction, action));
        var snapshot = Snapshotter.Capture();
        Write(new TraceEvent(0, "action_cancelled", action.Id, snapshot.Public, SemanticAction.From(action), null, snapshot.Oracle, null, null)
        {
            BeforePublicHash = snapshot.PublicHash,
            BeforeOracleHash = snapshot.OracleHash
        });
    });

    private void SettleOpenDecision(string boundary, CapturedSnapshot? suppliedSnapshot = null)
    {
        if (!Enabled) return;
        var snapshot = suppliedSnapshot ?? Snapshotter.Capture();
        SettleOpenDecisions(boundary, snapshot, nestedOnly: false);
    }

    private void SettleOpenDecisions(
        string boundary,
        CapturedSnapshot snapshot,
        bool nestedOnly)
    {
        PendingDecision[] pending;
        lock (_decisionLock)
        {
            pending = _openDecisions
                .Where(candidate => !nestedOnly || candidate.IsNested)
                .ToArray();
            _openDecisions.RemoveAll(candidate => !nestedOnly || candidate.IsNested);
        }
        foreach (var decision in pending) WriteDecision(decision, snapshot, boundary);
    }

    private void WriteDecision(PendingDecision pending, CapturedSnapshot after, string boundary)
    {
        int decisionIndex;
        lock (_decisionLock) decisionIndex = _decisionIndex++;
        Write(new TraceEvent(0, "decision", pending.GameActionId, pending.Before, pending.Action, after.Public,
            pending.BeforeOracle, after.Oracle, new Dictionary<string, object?>
            {
                ["decision_index"] = decisionIndex,
                ["settled_at"] = boundary,
                ["nested"] = pending.IsNested
            })
        {
            BeforePublicHash = pending.BeforePublicHash,
            BeforeOracleHash = pending.BeforeOracleHash,
            AfterPublicHash = after.PublicHash,
            AfterOracleHash = after.OracleHash
        });
    }

    public void OnRewardsSetBegun(MegaCrit.Sts2.Core.Rewards.RewardsSet set) =>
        Guard("rewards offered", () => _rewardsSet = set);

    public void OnCardChoiceOffered(
        IEnumerable<CardModel> options,
        int minimum,
        int maximum,
        bool canSkip = false,
        IReadOnlyList<string>? alternatives = null)
    {
        Guard("card choice offered", () =>
        {
            var candidates = options.Select(SemanticAction.CardIdentity).ToArray();
            if (candidates.Length == 0)
            {
                _choiceOffer = null;
                return;
            }
            if (minimum < 0 || maximum < minimum || maximum > candidates.Length)
                throw new InvalidOperationException(
                    $"Invalid card-choice range {minimum}..{maximum} for {candidates.Length} candidates.");
            var choice = new SemanticAction(
                "choose_cards",
                "choose-cards",
                null,
                null,
                "PlayerChoiceResult",
                CandidateIds: candidates,
                Minimum: minimum,
                Maximum: maximum,
                CanSkip: canSkip);
            var alternateActions = (alternatives ?? [])
                .Select((id, index) => id.Equals("Skip", StringComparison.OrdinalIgnoreCase)
                    ? null
                    : new SemanticAction(
                        "card_reward_alternative", $"alternative:{index}:{id}", id, null,
                        "MegaCrit.Sts2.Core.Entities.CardRewardAlternatives.CardRewardAlternative"))
                .ToArray();
            var legal = new[] { choice }.Concat(alternateActions.OfType<SemanticAction>()).ToArray();
            _choiceOffer = new("choose_cards", legal, candidates, minimum, maximum, canSkip,
                alternateActions);
            var snapshot = Snapshotter.Capture(legal);
            Resynchronize(snapshot, "card_choice");
            Write(new TraceEvent(0, "choice_offered", null,
                snapshot.Public, null, null,
                snapshot.Oracle, null,
                new Dictionary<string, object?>
                {
                    ["minimum"] = minimum,
                    ["maximum"] = maximum,
                    ["can_skip"] = canSkip,
                    ["candidate_ids"] = candidates
                })
            {
                BeforePublicHash = snapshot.PublicHash,
                BeforeOracleHash = snapshot.OracleHash
            });
        });
    }

    public void OnPlayerChoice(PlayerChoiceResult result, uint choiceId)
    {
        Guard("player choice", () =>
        {
            if (result.ChoiceType == MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.None) return;
            var offer = _choiceOffer ?? throw new InvalidOperationException(
                "A local player choice was submitted without a captured legal offer.");
            if (offer.ChoiceId is null)
            {
                offer = offer with { ChoiceId = choiceId };
                _choiceOffer = offer;
            }
            if (offer.ChoiceId != choiceId)
                throw new InvalidOperationException(
                    $"Player choice {choiceId} does not match reserved offer {offer.ChoiceId?.ToString() ?? "<none>"}.");
            var indexes = result.ChoiceType == MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.Index
                ? result.AsIndexes().ToArray()
                : [];
            SemanticAction action;
            if (indexes is [var index] && index >= offer.CandidateIds.Length)
            {
                var alternative = index - offer.CandidateIds.Length;
                if (alternative < 0 || alternative >= offer.AlternativeActions.Length)
                    throw new InvalidOperationException("Selected card-reward alternative is outside the captured offer.");
                action = offer.AlternativeActions[alternative]
                    ?? offer.LegalActions[0] with { SelectedIds = [] };
            }
            else
            {
                var selected = SemanticAction.SelectedChoiceIds(result, offer.CandidateIds);
                if (selected.Length < offer.Minimum || selected.Length > offer.Maximum ||
                    selected.Distinct(StringComparer.Ordinal).Count() != selected.Length ||
                    selected.Any(id => !offer.CandidateIds.Contains(id, StringComparer.Ordinal)))
                    throw new InvalidOperationException("Selected cards are outside the captured legal choice schema.");
                action = offer.LegalActions[0] with { SelectedIds = selected };
            }
            var snapshot = Snapshotter.Capture(offer.LegalActions);
            SettleOpenDecisions("next_nested_choice", snapshot, nestedOnly: true);
            lock (_decisionLock)
                _openDecisions.Add(new(
                    result,
                    choiceId,
                    action,
                    snapshot.Public,
                    snapshot.Oracle,
                    snapshot.PublicHash,
                    snapshot.OracleHash,
                    IsNested: true));
            _choiceOffer = null;
        });
    }

    public void BeginExternalDecision(
        object runtimeToken,
        object? runtimeId,
        SemanticAction chosen,
        IReadOnlyList<SemanticAction> legalActions)
    {
        Guard(chosen.Kind, () =>
        {
            if (legalActions.Count == 0 || legalActions.Count(action =>
                    action.Kind == chosen.Kind && action.Id == chosen.Id) != 1)
                throw new InvalidOperationException(
                    $"Chosen strategic action '{chosen.Id}' is not unique in its legal action set.");
            var snapshot = Snapshotter.Capture(legalActions.ToArray());
            Resynchronize(snapshot, chosen.Kind);
            SettleOpenDecisions("next_player_decision", snapshot, nestedOnly: false);
            lock (_decisionLock)
                _openDecisions.Add(new(
                    runtimeToken,
                    runtimeId,
                    chosen,
                    snapshot.Public,
                    snapshot.Oracle,
                    snapshot.PublicHash,
                    snapshot.OracleHash,
                    IsNested: false));
        });
    }

    public void CompleteEpisode(bool? won, string outcome)
    {
        Guard("episode completion", () =>
        {
            if (_completed) return;
            var snapshot = Snapshotter.Capture([]);
            Resynchronize(snapshot, "terminal");
            SettleOpenDecisions("terminal", snapshot, nestedOnly: false);
            Write(new TraceEvent(0, "episode_completed", null,
                snapshot.Public, null, snapshot.Public,
                snapshot.Oracle, snapshot.Oracle,
                new Dictionary<string, object?>
                {
                    ["complete"] = won.HasValue,
                    ["won"] = won,
                    ["outcome"] = outcome
                })
            {
                BeforePublicHash = snapshot.PublicHash,
                AfterPublicHash = snapshot.PublicHash,
                BeforeOracleHash = snapshot.OracleHash,
                AfterOracleHash = snapshot.OracleHash
            });
            _completed = true;
        });
    }

    public void RecordIncompleteShutdown()
    {
        if (_completed || !Enabled) return;
        try
        {
            Write(new TraceEvent(0, "episode_incomplete", null,
                null, null, null, null, null,
                new Dictionary<string, object?>
                {
                    ["complete"] = false,
                    ["reason"] = "process_exit_without_terminal_result"
                }));
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(
                $"[Spirefysh] Could not record incomplete shutdown: {exception.Message}");
        }
    }

    // Generic arguments are inferred from the game's public event at compile time.
    public void OnChecksumGenerated(NetChecksumData checksum, string name, NetFullCombatState state) => Guard("official checksum", () =>
        Write(new TraceEvent(0, "official_checksum", checksum.id, null, null, null, null, null, new Dictionary<string, object?> { ["name"] = name, ["checksum"] = checksum.checksum, ["state"] = state.ToString() })));

    public void RecordLifecycle(string kind, bool captureSnapshot = true)
    {
        if (captureSnapshot) WriteSnapshotEvent(kind);
        else Write(new TraceEvent(0, kind, null, null, null, null, null, null, null));
    }

    internal void RecordOracleCommandWave112(OracleCommandPhaseEventWave112 item) =>
        Guard("wave112 oracle command", () => Write(new TraceEvent(
            0, "oracle_command_phase", null, null, null, null, null, null,
            new Dictionary<string, object?> { ["oracle_command"] = item })));

    public void Disable(string reason)
    {
        if (!Enabled) return;
        Enabled = false;
        try { Write(new TraceEvent(0, "abstained", null, null, null, null, null, null, new Dictionary<string, object?> { ["reason"] = reason })); }
        catch (Exception writeException) { Console.Error.WriteLine($"[Spirefysh] Could not record abstention: {writeException.Message}"); }
        Console.Error.WriteLine($"[Spirefysh] Telemetry disabled: {reason}");
    }

    public void CaptureGap(string context, Exception exception) =>
        CaptureGap(context, $"{exception.GetType().Name}: {exception.Message}");

    public void CaptureGap(string context, string reason)
    {
        if (!Enabled) return;
        PendingDecision[] quarantined;
        int gapId;
        int faultIndex;
        lock (_decisionLock)
        {
            gapId = _activeGapId ??= _nextGapId++;
            faultIndex = _gapFaultIndex++;
            quarantined = _openDecisions.ToArray();
            _openDecisions.Clear();
            _choiceOffer = null;
            _rewardsSet = null;
        }
        try
        {
            Write(new TraceEvent(0, "capture_gap", null, null, null, null, null, null,
                new Dictionary<string, object?>
                {
                    ["gap_id"] = gapId,
                    ["fault_index"] = faultIndex,
                    ["context"] = context,
                    ["reason"] = reason,
                    ["quarantined_transition_count"] = quarantined.Length
                }));
            foreach (var pending in quarantined)
                Write(new TraceEvent(0, "transition_quarantined", pending.GameActionId,
                    pending.Before, pending.Action, null, pending.BeforeOracle, null,
                    new Dictionary<string, object?>
                    {
                        ["gap_id"] = gapId,
                        ["context"] = context,
                        ["reason"] = reason
                    })
                {
                    BeforePublicHash = pending.BeforePublicHash,
                    BeforeOracleHash = pending.BeforeOracleHash
                });
            Console.Error.WriteLine($"[Spirefysh] Capture gap {gapId}: {context}: {reason}");
        }
        catch (Exception writeException)
        {
            Disable($"trace output failure: {writeException.GetType().Name}: {writeException.Message}");
        }
    }

    private void Resynchronize(CapturedSnapshot snapshot, string boundary)
    {
        if (_activeGapId is not { } gapId) return;
        Write(new TraceEvent(0, "capture_resynchronized", null,
            snapshot.Public, null, null, snapshot.Oracle, null,
            new Dictionary<string, object?>
            {
                ["gap_id"] = gapId,
                ["boundary"] = boundary
            })
        {
            BeforePublicHash = snapshot.PublicHash,
            BeforeOracleHash = snapshot.OracleHash
        });
        _activeGapId = null;
        _gapFaultIndex = 0;
    }

    private void Guard(string context, Action operation)
    {
        if (!Enabled) return;
        try { operation(); }
        catch (TraceOutputException exception) { Disable($"trace output failure: {exception.InnerException?.Message ?? exception.Message}"); }
        catch (Exception exception) { CaptureGap(context, exception); }
    }

    private void Write(TraceEvent item)
    {
        try
        {
            lock (_writeLock)
            {
                _writer.Write(item, (candidate, sequence) => candidate with
                {
                    Sequence = sequence,
                    SchemaVersion = TraceSchemaVersion,
                    SessionId = _sessionId,
                    EpisodeId = _episodeId,
                    BuildCommit = BridgeEntry.SupportedCommit,
                    GameAssemblySha256 = BridgeEntry.SupportedAssemblySha256,
                    ContentManifestSha256 = ContentManifestSha256,
                    ContentArchiveSha256 = BridgeEntry.SupportedContentArchiveSha256,
                    BridgeSha256 = _bridgeSha256
                });
            }
        }
        catch (Exception exception) { throw new TraceOutputException(exception); }
    }

    private void WriteSnapshotEvent(string kind)
    {
        var snapshot = Snapshotter.Capture();
        WriteSnapshotEvent(kind, snapshot);
    }

    private void WriteSnapshotEvent(string kind, CapturedSnapshot snapshot)
    {
        Write(new TraceEvent(0, kind, null, snapshot.Public, null, null, snapshot.Oracle, null, null)
        {
            BeforePublicHash = snapshot.PublicHash,
            BeforeOracleHash = snapshot.OracleHash
        });
    }

    public void Dispose() { Enabled = false; _writer.Dispose(); }
    private sealed record PendingDecision(
        object RuntimeAction,
        object? GameActionId,
        SemanticAction Action,
        PublicSnapshot Before,
        OracleSnapshot BeforeOracle,
        string BeforePublicHash,
        string BeforeOracleHash,
        bool IsNested);

    private sealed record ChoiceOffer(
        string Kind,
        SemanticAction[] LegalActions,
        string[] CandidateIds,
        int Minimum,
        int Maximum,
        bool CanSkip,
        SemanticAction?[] AlternativeActions,
        uint? ChoiceId = null);

    private sealed class TraceOutputException(Exception inner) : Exception("Trace output failed.", inner);
}

internal sealed record TraceEvent(long Sequence, string Kind, object? GameActionId, PublicSnapshot? Before, SemanticAction? Action, PublicSnapshot? After, OracleSnapshot? OracleBefore, OracleSnapshot? OracleAfter, IReadOnlyDictionary<string, object?>? Metadata)
{
    public uint SchemaVersion { get; init; }
    public string SessionId { get; init; } = "";
    public string EpisodeId { get; init; } = "";
    public string BuildCommit { get; init; } = "";
    public string GameAssemblySha256 { get; init; } = "";
    public string ContentManifestSha256 { get; init; } = "";
    public string ContentArchiveSha256 { get; init; } = "";
    public string BridgeSha256 { get; init; } = "";
    public string? BeforePublicHash { get; init; }
    public string? AfterPublicHash { get; init; }
    public string? BeforeOracleHash { get; init; }
    public string? AfterOracleHash { get; init; }
}

internal sealed record SemanticAction(
    string Kind,
    string Id,
    string? SourceId,
    string? TargetId,
    string RuntimeType,
    IReadOnlyList<string>? CandidateIds = null,
    IReadOnlyList<string>? SelectedIds = null,
    int? Minimum = null,
    int? Maximum = null,
    bool CanSkip = false)
{
    public static SemanticAction From(GameAction action) => action switch
    {
        PlayCardAction play => CardAction(play, action.GetType().FullName!),
        UsePotionAction potion => new("use_potion", $"potion:{potion.PotionIndex}:{potion.TargetId}", $"potion-slot:{potion.PotionIndex}", CreatureId(potion.TargetId), action.GetType().FullName!),
        DiscardPotionGameAction discard => DiscardPotionAction(discard),
        EndPlayerTurnAction => new("end_turn", "end-turn", null, null, action.GetType().FullName!),
        UndoEndPlayerTurnAction => new("undo_end_turn", "undo-end-turn", null, null, action.GetType().FullName!),
        MoveToMapCoordAction move => MapAction(move),
        PickRelicAction pick => RelicAction(pick),
        _ => new("unknown", action.ToString() ?? action.GetType().Name, null, null, action.GetType().FullName!)
    };

    internal static SemanticAction[]? LegalActionsFor(GameAction action)
    {
        if (action is PlayCardAction or UsePotionAction or EndPlayerTurnAction) return null;
        if (action is DiscardPotionGameAction)
        {
            var run = RunManager.Instance.DebugOnlyGetState()
                ?? throw new InvalidOperationException("Run state is unavailable.");
            var player = run.GetPlayer(action.OwnerId)
                ?? throw new InvalidOperationException("Discard-potion owner is unavailable.");
            return player.Potions.Select(potion =>
            {
                var slot = player.GetPotionSlotIndex(potion);
                return new SemanticAction(
                    "discard_potion", $"discard-potion:{slot}",
                    $"potion-slot:{slot}", null,
                    typeof(DiscardPotionGameAction).FullName!);
            }).ToArray();
        }
        if (action is UndoEndPlayerTurnAction)
            return [From(action)];
        if (action is MoveToMapCoordAction)
        {
            var run = RunManager.Instance.DebugOnlyGetState()
                ?? throw new InvalidOperationException("Run state is unavailable.");
            var points = run.CurrentMapPoint is { } current
                ? MapTravel.GetTravelablePointsFrom(run, current)
                : run.Map.startMapPoints;
            return points.Select(MapAction).ToArray();
        }
        if (action is PickRelicAction)
        {
            var relics = RunManager.Instance.TreasureRoomRelicSynchronizer.CurrentRelics ?? [];
            return relics.Select((relic, index) => new SemanticAction(
                    "pick_relic", $"pick-relic:{index}:{relic.Id}",
                    relic.Id.ToString(), null, typeof(PickRelicAction).FullName!))
                .Append(new(
                    "skip_relic", "pick-relic:skip", null, null,
                    typeof(PickRelicAction).FullName!))
                .ToArray();
        }
        return [From(action)];
    }

    private static SemanticAction CardAction(PlayCardAction play, string runtimeType)
    {
        var source = $"combat-card:{play.NetCombatCard.CombatCardIndex}";
        var target = CreatureId(play.TargetId);
        return new("play_card", target is null ? $"play:{source}" : $"play:{source}:{target}", source, target, runtimeType);
    }

    private static string? CreatureId(object? id) => id is null ? null : $"creature:{id}";

    private static SemanticAction DiscardPotionAction(DiscardPotionGameAction action)
    {
        var slot = ReadField<uint>(action, "_potionSlotIndex");
        return new(
            "discard_potion", $"discard-potion:{slot}",
            $"potion-slot:{slot}", null, action.GetType().FullName!);
    }

    private static SemanticAction MapAction(MoveToMapCoordAction action) =>
        MapAction(ReadField<MapCoord>(action, "_destination"));

    private static SemanticAction MapAction(MegaCrit.Sts2.Core.Map.MapPoint point) =>
        MapAction(point.coord);

    private static SemanticAction MapAction(MapCoord coord) => new(
        "map_node", $"map:{coord.col}:{coord.row}",
        null, $"map:{coord.col}:{coord.row}",
        typeof(MoveToMapCoordAction).FullName!);

    private static SemanticAction RelicAction(PickRelicAction action)
    {
        var index = ReadField<int?>(action, "_relicIndex");
        if (index is null)
            return new(
                "skip_relic", "pick-relic:skip", null, null,
                action.GetType().FullName!);
        var relics = RunManager.Instance.TreasureRoomRelicSynchronizer.CurrentRelics ?? [];
        if (index < 0 || index >= relics.Count)
            throw new InvalidOperationException("Picked relic index is outside the visible offer.");
        return new(
            "pick_relic", $"pick-relic:{index}:{relics[index.Value].Id}",
            relics[index.Value].Id.ToString(), null, action.GetType().FullName!);
    }

    private static T ReadField<T>(object instance, string name)
    {
        var field = instance.GetType().GetField(
            name, BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingFieldException(instance.GetType().FullName, name);
        return (T)field.GetValue(instance)!;
    }

    internal static string CardIdentity(CardModel card)
    {
        try
        {
            return $"combat-card:{NetCombatCard.FromModel(card).CombatCardIndex}";
        }
        catch (Exception)
        {
            var run = RunManager.Instance.DebugOnlyGetState();
            if (run is not null)
                foreach (var player in run.Players)
                    for (var index = 0; index < player.Deck.Cards.Count; index++)
                        if (ReferenceEquals(player.Deck.Cards[index], card))
                            return $"deck-card:{player.NetId}:{index}";
            return $"model-card:{card.Id}:{card.CurrentUpgradeLevel}";
        }
    }

    internal static string[] SelectedChoiceIds(
        PlayerChoiceResult result,
        IReadOnlyList<string> candidateIds) =>
        result.ChoiceType switch
        {
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.CanonicalCard =>
                result.AsCanonicalCards().Select(CardIdentity).ToArray(),
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.CombatCard =>
                result.AsCombatCards().Select(CardIdentity).ToArray(),
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.DeckCard =>
                result.AsDeckCards().Select(CardIdentity).ToArray(),
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.MutableCard =>
                result.AsMutableCards().Select(CardIdentity).ToArray(),
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.Index =>
                result.AsIndexes().Select(index => index >= 0 && index < candidateIds.Count
                    ? candidateIds[index]
                    : throw new InvalidOperationException(
                        $"Selected index {index} is outside {candidateIds.Count} candidates.")).ToArray(),
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.Player =>
                result.AsPlayerId() is { } playerId ? [$"player:{playerId}"] : [],
            MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.None => [],
            var type => throw new InvalidOperationException(
                $"Unsupported player-choice result type '{type}'.")
        };
}

internal static class Snapshotter
{
    public static CapturedSnapshot Capture(
        SemanticAction[]? legalActions = null,
        SemanticAction? selectedAction = null) => CaptureInternal(null, legalActions, selectedAction);
    public static CapturedSnapshot Capture(CombatState combat) => CaptureInternal(combat, null, null);

    private static CapturedSnapshot CaptureInternal(
        CombatState? combatOverride,
        SemanticAction[]? legalActions,
        SemanticAction? selectedAction)
    {
        var manager = RunManager.Instance;
        var run = manager.DebugOnlyGetState() ?? throw new InvalidOperationException("RunState is unavailable at a decision boundary.");
        var combat = combatOverride;
        if (combat is null && CombatManager.Instance.IsInProgress) combat = CombatManager.Instance.DebugOnlyGetState();
        var publicSnapshot = new PublicSnapshot(
            TraceCapture.TraceSchemaVersion,
            BridgeEntry.SupportedCommit,
            run.AscensionLevel,
            run.CurrentActIndex + 1,
            run.ActFloor,
            run.TotalFloor,
            run.GameMode.ToString(),
            run.IsGameOver,
            run.Players.Select(player => CapturePlayer(player, combat, legalActions, selectedAction)).ToArray(),
            combat is null ? null : CaptureCombat(combat),
            CapturePhase(run, combat),
            run.CurrentRoom?.RoomType.ToString(),
            run.CurrentRoom is { } currentRoom ? currentRoom.ModelId?.ToString() : null,
            run.CurrentMapCoord is { } currentCoord
                ? new MapCoordSnapshot(currentCoord.col, currentCoord.row)
                : null,
            CaptureMap(run),
            run.MapPointHistory.SelectMany((floor, floorIndex) => floor.Select((entry, entryIndex) =>
                new HistorySnapshot(
                    floorIndex,
                    entryIndex,
                    entry.GetType().FullName ?? entry.GetType().Name,
                    entry.ToString() ?? entry.GetType().Name)))
                .ToArray(),
            legalActions ?? []);
        var hiddenDrawOrder = run.Players.Count == 1 && run.Players[0].PlayerCombatState is { } playerCombatState
            ? CaptureCombatCards(
                playerCombatState.DrawPile.Cards,
                requireCombatId: true)
            : [];
        var cardDeckVersionIndices = CaptureCardDeckVersionIndices(run.Players);
        var enemyMoveIds = combat is null
            ? new SortedDictionary<string, string?>(StringComparer.Ordinal)
            : new SortedDictionary<string, string?>(combat.Enemies.ToDictionary(
                enemy => $"creature:{enemy.CombatId}",
                enemy => enemy.Monster?.NextMove?.Id,
                StringComparer.Ordinal), StringComparer.Ordinal);
        var powerStates = new SortedDictionary<string, PowerOracleSnapshot[]>(StringComparer.Ordinal);
        foreach (var player in run.Players)
        {
            powerStates[$"creature:{player.Creature.CombatId}"] = CaptureOraclePowers(player.Creature.Powers);
            foreach (var pet in player.Creature.Pets)
                powerStates[$"creature:{pet.CombatId}"] = CaptureOraclePowers(pet.Powers);
        }
        if (combat is not null)
            foreach (var enemy in combat.Enemies) powerStates[$"creature:{enemy.CombatId}"] = CaptureOraclePowers(enemy.Powers);
        var (rngSeeds, rngCounters) = CaptureRng(run);
        var oracleSnapshot = new OracleSnapshot(
            rngSeeds,
            rngCounters,
            hiddenDrawOrder,
            enemyMoveIds,
            powerStates,
            cardDeckVersionIndices,
            combat is null ? 0U : CaptureNextCombatCardId());
        return new(
            publicSnapshot,
            oracleSnapshot,
            CanonicalHash(publicSnapshot),
            CanonicalHash(oracleSnapshot));
    }

    private static string CanonicalHash<T>(T value)
    {
        var bytes = JsonSerializer.SerializeToUtf8Bytes(value, TraceJson.Options);
        return Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
    }

    private static PlayerSnapshot CapturePlayer(
        Player player,
        CombatState? combat,
        SemanticAction[]? legalActions,
        SemanticAction? selectedAction)
    {
        var creature = player.Creature;
        var combatState = player.PlayerCombatState;
        var actions = legalActions ?? (combat is null || combatState is null ? [] : CaptureLegalActions(player, combat));
        if (selectedAction is not null && !actions.Any(candidate =>
                candidate.Kind == selectedAction.Kind && candidate.Id == selectedAction.Id &&
                candidate.SourceId == selectedAction.SourceId && candidate.TargetId == selectedAction.TargetId))
            actions = [.. actions, selectedAction];
        return new(
            player.NetId,
            $"creature:{creature.CombatId}",
            player.Character.Id.ToString(),
            creature.CurrentHp,
            creature.MaxHp,
            creature.Block,
            player.Gold,
            combatState?.Energy ?? 0,
            combatState?.MaxEnergy ?? 0,
            combatState?.Stars ?? 0,
            combatState?.TurnNumber ?? 0,
            CaptureDeckCards(player.Deck.Cards),
            combatState is null ? [] : CaptureCombatCards(
                combatState.Hand.Cards, requireCombatId: true),
            combatState is null ? [] : CaptureCombatCards(
                combatState.PlayPile.Cards, requireCombatId: true),
            combatState?.DrawPile.Cards.Count ?? 0,
            combatState is null ? [] : CaptureCombatCards(
                combatState.DiscardPile.Cards, requireCombatId: true),
            combatState is null ? [] : CaptureCombatCards(
                combatState.ExhaustPile.Cards, requireCombatId: true),
            player.Relics.Select(x => new EntitySnapshot(x.Id.ToString(), x.StackCount, x.Status.ToString())).ToArray(),
            player.Potions.Select(x => new EntitySnapshot(x.Id.ToString(), player.GetPotionSlotIndex(x), x.Usage.ToString())).ToArray(),
            creature.Pets.Select(CaptureSummon).ToArray(),
            CapturePowers(creature.Powers),
            CaptureCardsExhaustedThisTurn(combat),
            CapturePlayerLostHpThisTurn(combat, creature),
            CapturePlayerHpLossEventsThisCombat(combat, creature),
            CaptureEnergySpentThisTurn(combat, player),
            CaptureOstyAttacksThisTurn(combat, player),
            actions);
    }

    private static string CapturePhase(RunState run, CombatState? combat)
    {
        if (combat is not null) return "combat";
        return run.CurrentRoom?.RoomType.ToString() switch
        {
            "Event" => "event",
            "RestSite" => "rest_site",
            "Merchant" => "shop",
            "Treasure" => "treasure",
            var room when room is not null => room.ToLowerInvariant(),
            _ => "map"
        };
    }

    private static MapNodeSnapshot[] CaptureMap(RunState run) =>
        run.Map.GetAllMapPoints()
            .OrderBy(point => point.coord.row)
            .ThenBy(point => point.coord.col)
            .Select(point => new MapNodeSnapshot(
                point.coord.col,
                point.coord.row,
                point.PointType.ToString(),
                point.Children.OrderBy(child => child.coord.row).ThenBy(child => child.coord.col)
                    .Select(child => new MapCoordSnapshot(child.coord.col, child.coord.row))
                    .ToArray(),
                point.Quests.Select(quest => quest.Id.ToString()).Order(StringComparer.Ordinal).ToArray(),
                run.VisitedMapCoords.Contains(point.coord)))
            .ToArray();

    private static SummonSnapshot CaptureSummon(Creature pet)
    {
        if (pet.CombatId is null)
            throw new InvalidOperationException($"Pet '{pet.ModelId}' has no combat identity.");
        return new(
            $"creature:{pet.CombatId}",
            pet.ModelId.ToString(),
            pet.CurrentHp,
            pet.MaxHp,
            pet.Block,
            CapturePowers(pet.Powers));
    }

    private static CombatSnapshot CaptureCombat(CombatState combat) => new(
        combat.RoundNumber,
        combat.CurrentSide.ToString(),
        combat.Enemies.Select(enemy => new EnemySnapshot(
            $"creature:{enemy.CombatId}",
            enemy.ModelId.ToString(),
            enemy.CurrentHp,
            enemy.MaxHp,
            enemy.Block,
            CaptureIntents(combat, enemy),
            CapturePowers(enemy.Powers))).ToArray());

    private static IntentSnapshot[] CaptureIntents(CombatState combat, Creature enemy) =>
        enemy.Monster?.NextMove?.Intents.Select(intent => intent is AttackIntent attack
            ? new IntentSnapshot(attack.IntentType.ToString(), attack.GetSingleDamage(combat.Creatures, enemy), attack.Repeats)
            : new IntentSnapshot(intent.IntentType.ToString(), 0, 0)).ToArray() ?? [];

    private static CardSnapshot[] CaptureDeckCards(IReadOnlyList<CardModel> cards) =>
        cards.Select((card, index) => CaptureCard(
            card,
            $"deck-card:{index}")).ToArray();

    private static CardSnapshot[] CaptureCombatCards(
        IReadOnlyList<CardModel> cards,
        bool requireCombatId) => cards.Select(card => CaptureCard(
            card,
            $"combat-card:{CaptureCombatCardIndex(card, requireCombatId)}")).ToArray();

    private static CardSnapshot CaptureCard(
        CardModel card,
        string instanceId)
    {
        var (costForTurn, combatCostOverride) = CaptureLocalAbsoluteCosts(card);
        var (enchantmentModelId, enchantmentAmount, enchantmentCombatAmount) =
            CaptureEnchantment(card);
        return new CardSnapshot(
            instanceId,
            card.Id.ToString(),
            card.CurrentUpgradeLevel,
            card.EnergyCost.GetResolved(),
            card.CurrentStarCost,
            card.Type.ToString(),
            card.TargetType.ToString(),
            CaptureRampageExtraDamage(card),
            CaptureClawExtraDamage(card),
            CaptureGeneticAlgorithmBonusBlock(card),
            card.BaseReplayCount,
            card.EnergyCost.CostsX,
            costForTurn,
            combatCostOverride,
            enchantmentModelId,
            enchantmentAmount,
            enchantmentCombatAmount,
            card.Affliction?.Id.ToString(),
            CaptureLocalCostModifiers(card));
    }

    private static (string? ModelId, decimal? Amount, decimal CombatAmount) CaptureEnchantment(
        CardModel card)
    {
        var enchantment = card.Enchantment;
        if (enchantment is null) return (null, null, 0m);
        var type = enchantment.GetType();
        var amountProperty = type.GetProperty(
                "Amount", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? type.BaseType?.GetProperty(
                "Amount", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(type.FullName, "Amount");
        var amount = Convert.ToDecimal(amountProperty.GetValue(enchantment)
            ?? throw new InvalidOperationException(
                $"Enchantment '{enchantment.Id}' amount is unavailable."));
        var combatAmount = 0m;
        if (string.Equals(enchantment.Id.ToString(), "ENCHANTMENT.MOMENTUM", StringComparison.Ordinal))
        {
            var extraDamage = type.GetProperty(
                    "ExtraDamage", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
                ?? throw new MissingMemberException(type.FullName, "ExtraDamage");
            combatAmount = Convert.ToDecimal(extraDamage.GetValue(enchantment)
                ?? throw new InvalidOperationException("Momentum extra damage is unavailable."));
        }
        return (enchantment.Id.ToString(), amount, combatAmount);
    }

    private static int? CaptureDeckVersionIndex(
        CardModel card,
        IReadOnlyList<CardModel> masterDeck)
    {
        if (card.DeckVersion is null) return null;
        for (var index = 0; index < masterDeck.Count; index++)
            if (ReferenceEquals(masterDeck[index], card.DeckVersion)) return index;
        throw new InvalidOperationException(
            $"Card '{card.Id}' DeckVersion is absent from the represented player deck.");
    }

    private static IReadOnlyDictionary<string, int?> CaptureCardDeckVersionIndices(
        IReadOnlyList<Player> players)
    {
        var result = new SortedDictionary<string, int?>(StringComparer.Ordinal);
        foreach (var player in players)
        {
            if (player.PlayerCombatState is not { } combatState) continue;
            foreach (var card in combatState.DrawPile.Cards
                         .Concat(combatState.Hand.Cards)
                         .Concat(combatState.PlayPile.Cards)
                         .Concat(combatState.DiscardPile.Cards)
                         .Concat(combatState.ExhaustPile.Cards))
            {
                var id = $"combat-card:{CaptureCombatCardIndex(card, required: true)}";
                if (!result.TryAdd(id, CaptureDeckVersionIndex(card, player.Deck.Cards)))
                    throw new InvalidOperationException(
                        $"Combat card identity '{id}' occurs in more than one pile.");
            }
        }
        return result;
    }

    private static (int? CostForTurn, int? CombatCostOverride) CaptureLocalAbsoluteCosts(CardModel card)
    {
        var cost = card.EnergyCost;
        var field = cost.GetType().GetField("_localModifiers", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(cost.GetType().FullName, "_localModifiers");
        if (field.GetValue(cost) is not System.Collections.IEnumerable modifiers)
            throw new InvalidOperationException("Card local-cost modifiers are unavailable.");
        int? costForTurn = null;
        int? combatCostOverride = null;
        foreach (var modifier in modifiers)
        {
            if (modifier is null) continue;
            var type = modifier.GetType();
            var modifierType = type.GetProperty("Type", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier)?.ToString();
            var expiration = type.GetProperty("Expiration", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier)?.ToString();
            var reduceOnly = type.GetProperty("IsReduceOnly", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier) as bool?;
            var amountValue = type.GetProperty("Amount", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier);
            if (modifierType != "Absolute" || reduceOnly != false) continue;
            if (amountValue is null) throw new InvalidOperationException("Absolute card-cost modifier amount is unavailable.");
            var amount = Convert.ToInt32(amountValue);
            switch (expiration)
            {
                case "EndOfTurn": costForTurn = amount; break;
                case "EndOfCombat": combatCostOverride = amount; break;
                case "EndOfTurn, WhenPlayed": break;
                default:
                    throw new InvalidOperationException(
                        $"Unsupported absolute card-cost modifier expiration '{expiration ?? "<missing>"}'.");
            }
        }
        return (costForTurn, combatCostOverride);
    }

    private static CardCostModifierSnapshot[] CaptureLocalCostModifiers(CardModel card)
    {
        var cost = card.EnergyCost;
        var field = cost.GetType().GetField("_localModifiers", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(cost.GetType().FullName, "_localModifiers");
        if (field.GetValue(cost) is not System.Collections.IEnumerable modifiers)
            throw new InvalidOperationException("Card local-cost modifiers are unavailable.");
        var result = new List<CardCostModifierSnapshot>();
        foreach (var modifier in modifiers)
        {
            if (modifier is null) continue;
            var type = modifier.GetType();
            var modifierType = type.GetProperty("Type", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier)?.ToString()
                ?? throw new InvalidOperationException("Card-cost modifier type is unavailable.");
            var expirationValue = type.GetProperty("Expiration", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier)
                ?? throw new InvalidOperationException("Card-cost modifier expiration is unavailable.");
            var expiration = expirationValue.ToString();
            var reduceOnly = type.GetProperty("IsReduceOnly", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier) as bool?
                ?? throw new InvalidOperationException("Card-cost modifier reduce-only flag is unavailable.");
            var amount = Convert.ToInt32(type.GetProperty("Amount", BindingFlags.Instance | BindingFlags.Public)?.GetValue(modifier)
                ?? throw new InvalidOperationException("Card-cost modifier amount is unavailable."));
            if (modifierType == "Absolute" && !reduceOnly && expiration is "EndOfTurn" or "EndOfCombat")
                continue;
            result.Add(new(amount, modifierType, Convert.ToInt32(expirationValue), reduceOnly));
        }
        return result.ToArray();
    }

    private static int CaptureRampageExtraDamage(CardModel card)
    {
        if (card.Id.ToString() != "CARD.RAMPAGE") return 0;
        var property = card.GetType().GetProperty("ExtraDamageFromPlays", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(card.GetType().FullName, "ExtraDamageFromPlays");
        return decimal.ToInt32((decimal)(property.GetValue(card)
            ?? throw new InvalidOperationException("Rampage extra damage is unavailable.")));
    }

    private static int CaptureClawExtraDamage(CardModel card)
    {
        if (card.Id.ToString() != "CARD.CLAW") return 0;
        var property = card.GetType().GetProperty(
            "ExtraDamageFromClawPlays",
            BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(card.GetType().FullName, "ExtraDamageFromClawPlays");
        return decimal.ToInt32((decimal)(property.GetValue(card)
            ?? throw new InvalidOperationException("Claw extra damage is unavailable.")));
    }

    private static int CaptureGeneticAlgorithmBonusBlock(CardModel card)
    {
        if (card.Id.ToString() != "CARD.GENETIC_ALGORITHM") return 0;
        var property = card.GetType().GetProperty(
            "IncreasedBlock",
            BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(card.GetType().FullName, "IncreasedBlock");
        return Convert.ToInt32(property.GetValue(card)
            ?? throw new InvalidOperationException(
                "Genetic Algorithm persistent Block bonus is unavailable."));
    }

    private static int CaptureCardsExhaustedThisTurn(CombatState? combat) => combat is null ? 0 :
        CombatManager.Instance.History.Entries.OfType<CardExhaustedEntry>().Count(entry => entry.HappenedThisTurn(combat));

    private static bool CapturePlayerLostHpThisTurn(CombatState? combat, Creature player) => combat is not null &&
        CombatManager.Instance.History.Entries.OfType<DamageReceivedEntry>().Any(entry =>
            ReferenceEquals(entry.Receiver, player) && entry.Result.UnblockedDamage > 0 && entry.HappenedThisTurn(combat));

    private static int CapturePlayerHpLossEventsThisCombat(CombatState? combat, Creature player) => combat is null ? 0 :
        CombatManager.Instance.History.Entries.OfType<DamageReceivedEntry>().Count(entry =>
            ReferenceEquals(entry.Receiver, player) && entry.Result.UnblockedDamage > 0);

    private static int CaptureEnergySpentThisTurn(CombatState? combat, Player player) => combat is null ? 0 :
        CombatManager.Instance.History.Entries.OfType<EnergySpentEntry>().Where(entry =>
            ReferenceEquals(entry.Actor, player.Creature) && entry.HappenedThisTurn(combat)).Sum(entry => entry.Amount);

    private static int CaptureOstyAttacksThisTurn(CombatState? combat, Player player)
    {
        if (combat is null || player.Osty is null) return 0;
        var osty = player.Osty;
        return CombatManager.Instance.History.Entries.OfType<CreatureAttackedEntry>().Count(entry =>
            ReferenceEquals(entry.Actor, osty) && entry.HappenedThisTurn(combat));
    }

    private static string CaptureCombatCardIndex(CardModel card, bool required)
    {
        try { return MegaCrit.Sts2.Core.Entities.Multiplayer.NetCombatCard.FromModel(card).CombatCardIndex.ToString(); }
        catch (Exception) when (!required) { return $"deck:{card.Id}:{card.CurrentUpgradeLevel}"; }
    }

    private static uint CaptureNextCombatCardId()
    {
        var field = typeof(NetCombatCardDb).GetField("_nextId", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingFieldException(typeof(NetCombatCardDb).FullName, "_nextId");
        return (uint)(field.GetValue(NetCombatCardDb.Instance)
            ?? throw new InvalidOperationException("NetCombatCardDb._nextId is unavailable."));
    }

    private static EntitySnapshot[] CapturePowers(IReadOnlyList<PowerModel> powers) => powers.Select(x =>
        new EntitySnapshot(x.Id.ToString(), CaptureVisiblePowerAmount(x), x.Type.ToString())).ToArray();
    private static PowerOracleSnapshot[] CaptureOraclePowers(IReadOnlyList<PowerModel> powers) => powers.Select(x =>
        new PowerOracleSnapshot(x.Id.ToString(), x.Amount, x.AmountOnTurnStart, x.SkipNextDurationTick,
            CaptureDarkEmbraceEtherealCount(x), CaptureSelfDamage(x),
            CaptureIntProperty(x, "POWER.SLOTH_POWER", "CardsPlayedThisTurn"),
            CaptureAdvancedCounterDebuffActivityCount(x), CaptureBoundCardPlayed(x),
            CaptureFeralReturnsThisTurn(x), CaptureToricToughnessBlock(x))).ToArray();

    private static int CaptureToricToughnessBlock(PowerModel power)
    {
        if (power.Id.ToString() != "POWER.TORIC_TOUGHNESS_POWER") return 0;
        var property = power.GetType().GetProperty(
            "Block", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? throw new MissingMemberException(power.GetType().FullName, "Block");
        var value = Convert.ToDecimal(property.GetValue(power)
            ?? throw new InvalidOperationException("Toric Toughness Block is unavailable."));
        if (value <= 0 || value != decimal.Truncate(value))
            throw new InvalidOperationException($"Toric Toughness Block '{value}' is invalid.");
        return decimal.ToInt32(value);
    }

    private static int CaptureFeralReturnsThisTurn(PowerModel power)
    {
        if (power.Id.ToString() != "POWER.FERAL_POWER") return 0;
        var data = typeof(PowerModel).GetField("_internalData", BindingFlags.Instance | BindingFlags.NonPublic)!
            .GetValue(power) ?? throw new InvalidOperationException("Feral internal data is unavailable.");
        var field = data.GetType().GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            .Single(item => item.Name.Contains("zeroCostAttacksPlayed", StringComparison.OrdinalIgnoreCase));
        return (int)(field.GetValue(data)
            ?? throw new InvalidOperationException("Feral returned-card count is unavailable."));
    }

    private static bool CaptureBoundCardPlayed(PowerModel power)
    {
        if (power.Id.ToString() != "POWER.CHAINS_OF_BINDING_POWER") return false;
        var data = typeof(PowerModel).GetField("_internalData", BindingFlags.Instance | BindingFlags.NonPublic)!
            .GetValue(power) ?? throw new InvalidOperationException("Chains of Binding internal data is unavailable.");
        var field = data.GetType().GetFields(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            .Single(item => item.Name.Contains("boundCardPlayed", StringComparison.OrdinalIgnoreCase));
        return (bool)(field.GetValue(data)
            ?? throw new InvalidOperationException("Chains of Binding play state is unavailable."));
    }

    private static int CaptureVisiblePowerAmount(PowerModel power) =>
        power.Id.ToString() is "POWER.SLOW_POWER" or "POWER.TENDER_POWER"
            ? CaptureIntProperty(power, power.Id.ToString(), "DisplayAmount")
            : power.Amount;

    private static int CaptureDarkEmbraceEtherealCount(PowerModel power)
    {
        if (power.Id.ToString() != "POWER.DARK_EMBRACE_POWER") return 0;
        var dataField = typeof(PowerModel).GetField("_internalData", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new MissingFieldException(typeof(PowerModel).FullName, "_internalData");
        var data = dataField.GetValue(power)
            ?? throw new InvalidOperationException("Dark Embrace internal data is unavailable.");
        var countField = data.GetType().GetField("etherealCount", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? throw new MissingFieldException(data.GetType().FullName, "etherealCount");
        return (int)(countField.GetValue(data)
            ?? throw new InvalidOperationException("Dark Embrace Ethereal count is unavailable."));
    }

    private static int CaptureSelfDamage(PowerModel power) =>
        power.Id.ToString() is "POWER.CRIMSON_MANTLE_POWER" or "POWER.INFERNO_POWER"
            ? power.DynamicVars.Values.Single(value => value.Name == "SelfDamage").IntValue
            : 0;

    private static int CaptureAdvancedCounterDebuffActivityCount(PowerModel power)
    {
        var id = power.Id.ToString();
        if (id == "POWER.TENDER_POWER")
            return CaptureIntProperty(power, id, "CardsPlayedThisTurn");
        if (id != "POWER.SLOW_POWER") return 0;
        var display = CaptureIntProperty(power, id, "DisplayAmount");
        if (display < 0 || display % 10 != 0)
            throw new InvalidOperationException($"Slow display amount '{display}' cannot be captured exactly.");
        return display / 10;
    }

    private static int CaptureIntProperty(PowerModel power, string expectedId, string propertyName)
    {
        if (power.Id.ToString() != expectedId) return 0;
        var property = power.GetType().GetProperty(propertyName,
                           BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
                       ?? throw new MissingMemberException(power.GetType().FullName, propertyName);
        var value = property.GetValue(power);
        if (value is not int result || result < 0)
            throw new InvalidOperationException($"{expectedId} {propertyName} is unavailable or negative.");
        return result;
    }

    private static SemanticAction[] CaptureLegalActions(Player player, CombatState combat)
    {
        var actions = new List<SemanticAction>();
        if (CombatManager.Instance.IsInProgress && !CombatManager.Instance.IsOverOrEnding &&
            combat.CurrentSide.ToString().Contains("Player", StringComparison.OrdinalIgnoreCase) &&
            !CombatManager.Instance.PlayerActionsDisabled)
        {
            var combatState = player.PlayerCombatState ?? throw new InvalidOperationException("PlayerCombatState is unavailable.");
            foreach (var card in combatState.Hand.Cards)
            {
                if (!card.CanPlay()) continue;
                var source = $"combat-card:{CaptureCombatCardIndex(card, required: true)}";
                if (RequiresCreatureTarget(card.TargetType))
                {
                    foreach (var target in combat.Creatures.Where(card.CanPlayTargeting))
                        actions.Add(new("play_card", $"play:{source}:creature:{target.CombatId}", source, $"creature:{target.CombatId}", nameof(PlayCardAction)));
                }
                else actions.Add(new("play_card", $"play:{source}", source, null, nameof(PlayCardAction)));
            }
            foreach (var potion in player.Potions)
            {
                if (potion.IsQueued || potion.HasBeenRemovedFromState || !potion.PassesCustomUsabilityCheck) continue;
                var slot = player.GetPotionSlotIndex(potion);
                var source = $"potion-slot:{slot}";
                if (RequiresCreatureTarget(potion.TargetType))
                {
                    foreach (var target in combat.Creatures.Where(potion.IsValidTarget))
                        actions.Add(new(
                            "use_potion",
                            $"potion:{slot}:{target.CombatId}",
                            source,
                            $"creature:{target.CombatId}",
                            nameof(UsePotionAction)));
                }
                else
                    actions.Add(new("use_potion", $"potion:{slot}:", source, null, nameof(UsePotionAction)));
            }
            actions.Add(new("end_turn", "end-turn", null, null, nameof(EndPlayerTurnAction)));
        }
        return actions.ToArray();
    }

    private static bool RequiresCreatureTarget(TargetType targetType) => targetType is
        TargetType.AnyEnemy or TargetType.AnyAlly or TargetType.AnyPlayer or TargetType.Osty;

    private static (IReadOnlyDictionary<string, uint> Seeds, IReadOnlyDictionary<string, int> Counters) CaptureRng(RunState run)
    {
        var seeds = new SortedDictionary<string, uint>(StringComparer.Ordinal);
        var counters = new SortedDictionary<string, int>(StringComparer.Ordinal);
        AddRngProperties(seeds, counters, "run", run.Rng);
        foreach (var player in run.Players) AddRngProperties(seeds, counters, $"player.{player.NetId}", player.PlayerRng);
        return (seeds, counters);
    }

    private static void AddRngProperties(IDictionary<string, uint> seeds, IDictionary<string, int> counters, string prefix, object set)
    {
        foreach (var property in set.GetType().GetProperties(BindingFlags.Public | BindingFlags.Instance).OrderBy(x => x.Name, StringComparer.Ordinal))
            if (property.PropertyType == typeof(Rng) && property.GetValue(set) is Rng rng)
            {
                var key = $"{prefix}.{property.Name}";
                seeds[key] = rng.Seed;
                counters[key] = rng.Counter;
            }
    }
}

internal sealed record CapturedSnapshot(
    PublicSnapshot Public,
    OracleSnapshot Oracle,
    string PublicHash,
    string OracleHash);
internal sealed record PublicSnapshot(
    uint SchemaVersion,
    string BuildHash,
    int Ascension,
    int Act,
    int ActFloor,
    int TotalFloor,
    string GameMode,
    bool IsGameOver,
    PlayerSnapshot[] Players,
    CombatSnapshot? Combat,
    string Phase,
    string? RoomType,
    string? RoomModelId,
    MapCoordSnapshot? CurrentMapCoord,
    MapNodeSnapshot[] Map,
    HistorySnapshot[] History,
    SemanticAction[] VisibleOffers);
internal sealed record OracleSnapshot(IReadOnlyDictionary<string, uint> RngSeeds, IReadOnlyDictionary<string, int> RngCounters, CardSnapshot[] HiddenDrawOrder, IReadOnlyDictionary<string, string?> EnemyMoveIds, IReadOnlyDictionary<string, PowerOracleSnapshot[]> PowerStates, IReadOnlyDictionary<string, int?> CardDeckVersionIndices, uint NextCombatCardId);
internal sealed record PlayerSnapshot(ulong NetId, string InstanceId, string CharacterId, int HitPoints, int MaxHitPoints, int Block, int Gold, int Energy, int MaxEnergy, int Stars, int TurnNumber, CardSnapshot[] Deck, CardSnapshot[] Hand, CardSnapshot[] PlayPile, int DrawPileCount, CardSnapshot[] DiscardPile, CardSnapshot[] ExhaustPile, EntitySnapshot[] Relics, EntitySnapshot[] Potions, SummonSnapshot[] Summons, EntitySnapshot[] Powers, int CardsExhaustedThisTurn, bool PlayerLostHpThisTurn, int PlayerHpLossEventsThisCombat, int EnergySpentThisTurn, int OstyAttacksThisTurn, SemanticAction[] LegalActions);
internal sealed record CombatSnapshot(int Round, string Side, EnemySnapshot[] Enemies);
internal sealed record EnemySnapshot(string InstanceId, string ModelId, int HitPoints, int MaxHitPoints, int Block, IntentSnapshot[] VisibleIntents, EntitySnapshot[] Powers);
internal sealed record SummonSnapshot(string InstanceId, string ModelId, int HitPoints, int MaxHitPoints, int Block, EntitySnapshot[] Powers);
internal sealed record IntentSnapshot(string Kind, int Damage, int Hits);
internal sealed record CardSnapshot(
    string InstanceId,
    string ModelId,
    int Upgrade,
    int EnergyCost,
    int StarCost,
    string Type,
    string TargetType,
    int RampageExtraDamage,
    int ClawExtraDamage,
    int GeneticAlgorithmBonusBlock,
    int BaseReplayCount,
    bool CostsX,
    int? CostForTurn,
    int? CombatCostOverride,
    string? EnchantmentModelId,
    decimal? EnchantmentAmount,
    decimal EnchantmentCombatAmount,
    string? AfflictionModelId,
    CardCostModifierSnapshot[] CostModifiers);
internal sealed record CardCostModifierSnapshot(
    int Amount,
    string Type,
    int Expiration,
    bool IsReduceOnly);
internal sealed record EntitySnapshot(string ModelId, int Amount, string State);
internal sealed record PowerOracleSnapshot(string ModelId, int Amount, int AmountOnTurnStart, bool SkipNextDurationTick, int DarkEmbraceEtherealCount, int SelfDamage, int CardsPlayedThisTurn, int AdvancedCounterDebuffActivityCount, bool BoundCardPlayed, int FeralReturnsThisTurn, int ToricToughnessBlock);
internal sealed record MapCoordSnapshot(int Column, int Row);
internal sealed record MapNodeSnapshot(
    int Column,
    int Row,
    string PointType,
    MapCoordSnapshot[] Children,
    string[] QuestModelIds,
    bool Visited);
internal sealed record HistorySnapshot(
    int FloorIndex,
    int EntryIndex,
    string RuntimeType,
    string Summary);
