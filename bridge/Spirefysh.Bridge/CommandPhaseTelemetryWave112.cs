using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;
using HarmonyLib;
using MegaCrit.Sts2.Core.Combat;
using MegaCrit.Sts2.Core.Commands;
using MegaCrit.Sts2.Core.Entities.Creatures;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Models.Monsters;
using MegaCrit.Sts2.Core.Models.Powers;
using MegaCrit.Sts2.Core.Models.Relics;
using MegaCrit.Sts2.Core.ValueProps;

namespace Spirefysh.Bridge;

internal sealed record OracleCreatureWave112(
    string InstanceId,
    string ModelId,
    decimal HitPoints,
    decimal MaxHitPoints,
    decimal Block,
    bool InCombat);

internal sealed record OracleCalendarWave112(
    string OwnerInstanceId,
    int DisplayAmount,
    bool ShowCounter,
    bool IsActivating,
    string Status);

internal sealed record OracleCommandPhaseEventWave112(
    long EventOrder,
    long InvocationId,
    string Stage,
    string Operation,
    string RuntimeMethod,
    int? CombatRound,
    string? CombatSide,
    int? PlayerTurn,
    IReadOnlyList<OracleCalendarWave112> Calendars,
    decimal? RequestedAmount,
    int? ValueProps,
    string? ValuePropsText,
    string? DealerInstanceId,
    string? DealerModelId,
    string? SourceModelId,
    IReadOnlyList<OracleCreatureWave112> Targets,
    string? Error = null);

internal static class CommandPhaseTelemetryWave112
{
    private static Action<OracleCommandPhaseEventWave112>? _write;
    private static Action<string, Exception>? _gap;
    private static Func<bool>? _enabled;
    private static long _eventOrder;
    private static long _invocationId;

    internal static void Install(
        Harmony harmony,
        Action<OracleCommandPhaseEventWave112> write,
        Action<string, Exception> gap,
        Func<bool> enabled)
    {
        ArgumentNullException.ThrowIfNull(harmony);
        ArgumentNullException.ThrowIfNull(write);
        ArgumentNullException.ThrowIfNull(gap);
        ArgumentNullException.ThrowIfNull(enabled);
        if (Interlocked.CompareExchange(ref _write, write, null) is not null)
            throw new InvalidOperationException("Wave112 telemetry was installed more than once.");
        _gap = gap;
        _enabled = enabled;

        var damage = AccessTools.GetDeclaredMethods(typeof(CreatureCmd)).Where(method =>
        {
            var parameters = method.GetParameters();
            return method.Name == nameof(CreatureCmd.Damage) &&
                   parameters.Any(parameter => parameter.Name == "amount" && parameter.ParameterType == typeof(decimal)) &&
                   parameters.Any(parameter => parameter.Name == "props" && parameter.ParameterType == typeof(ValueProp)) &&
                   parameters.Any(parameter => parameter.Name == "dealer" && parameter.ParameterType == typeof(Creature)) &&
                   parameters.Any(parameter => parameter.Name == "cardSource" && parameter.ParameterType == typeof(CardModel));
        }).ToArray();
        if (damage.Length != 2)
            throw new MissingMethodException(typeof(CreatureCmd).FullName,
                $"two canonical decimal Damage overloads; found {damage.Length}");
        foreach (var method in damage) PatchAsync(harmony, method);
        PatchAsync(harmony, Required(typeof(CreatureCmd), nameof(CreatureCmd.Heal),
            [typeof(Creature), typeof(decimal), typeof(bool)]));
        PatchAsync(harmony, Required(typeof(CreatureCmd), nameof(CreatureCmd.SetCurrentHp),
            [typeof(Creature), typeof(decimal)]));

        var hpSetter = Required(typeof(Creature), nameof(Creature.SetCurrentHpInternal), [typeof(decimal)]);
        harmony.Patch(hpSetter,
            prefix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(BeforeMutation)),
            postfix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(AfterSync)));
        var removal = AccessTools.GetDeclaredMethods(typeof(CombatState)).Single(method =>
            method.Name == nameof(CombatState.RemoveCreature) &&
            method.GetParameters().Length == 2 &&
            method.GetParameters()[0].ParameterType == typeof(Creature) &&
            method.GetParameters()[1].ParameterType == typeof(bool));
        harmony.Patch(removal,
            prefix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(BeforeInstanceCommand)),
            postfix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(AfterSync)));

        PatchPhase(harmony, typeof(StoneCalendar), "BeforeSideTurnEnd");
        PatchPhase(harmony, typeof(StoneCalendar), "AfterSideTurnStart");
        PatchPhase(harmony, typeof(PlatingPower), "BeforeSideTurnEndEarly");
        PatchPhase(harmony, typeof(KinPriest), "AfterDeath");
        foreach (var name in new[]
                 { "EndPlayerTurnPhaseOneInternal", "DoTurnEnd", "EndPlayerTurnPhaseTwoInternal",
                     "SwitchFromPlayerToEnemySide", "StartTurn", "SetupPlayerTurn" })
            PatchPhase(harmony, typeof(CombatManager), name);
    }

    private static void PatchAsync(Harmony harmony, MethodInfo method) => harmony.Patch(method,
        prefix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(BeforeStaticCommand)),
        postfix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(AfterAsync)));

    private static void PatchPhase(Harmony harmony, Type type, string name)
    {
        var methods = AccessTools.GetDeclaredMethods(type).Where(method => method.Name == name).ToArray();
        if (methods.Length != 1) throw new MissingMethodException(type.FullName, $"{name} x1; found {methods.Length}");
        harmony.Patch(methods[0],
            prefix: new HarmonyMethod(typeof(CommandPhaseTelemetryWave112), nameof(BeforePhase)));
    }

    private static MethodInfo Required(Type type, string name, Type[] parameters) =>
        AccessTools.Method(type, name, parameters) ?? throw new MissingMethodException(type.FullName, name);

    private static void BeforeStaticCommand(
        MethodBase __originalMethod,
        object[] __args,
        out InvocationWave112? __state) =>
        __state = Begin(__originalMethod, null, __args, "request");

    private static void BeforeInstanceCommand(
        MethodBase __originalMethod,
        object __instance,
        object[] __args,
        out InvocationWave112? __state) =>
        __state = Begin(__originalMethod, __instance, __args, "request");

    private static void BeforeMutation(
        Creature __instance,
        MethodBase __originalMethod,
        object[] __args,
        out InvocationWave112? __state) =>
        __state = Begin(__originalMethod, __instance, __args, "mutation_request");

    private static void BeforePhase(MethodBase __originalMethod, object? __instance, object[] __args) =>
        _ = Begin(__originalMethod, __instance, __args, "phase_enter");

    private static void AfterAsync(object __result, InvocationWave112? __state)
    {
        if (__state is null || __result is not Task task) return;
        _ = task.ContinueWith(result => Complete(__state, result),
            CancellationToken.None, TaskContinuationOptions.ExecuteSynchronously, TaskScheduler.Default);
    }

    private static void AfterSync(InvocationWave112? __state)
    {
        if (__state is not null) Complete(__state, null);
    }

    private static InvocationWave112? Begin(
        MethodBase method,
        object? instance,
        IReadOnlyList<object?> args,
        string stage)
    {
        try
        {
            if (_enabled?.Invoke() != true) return null;
            var parameters = method.GetParameters();
            object? Argument(string name) => parameters.Select((parameter, index) => (parameter, index))
                .Where(pair => pair.parameter.Name == name).Select(pair => args[pair.index]).SingleOrDefault();
            var targets = Targets(instance, Argument("target") ?? Argument("targets") ??
                Argument("creature") ?? (instance is Creature ? instance : null));
            decimal? amount = Argument("amount") is { } value ? Convert.ToDecimal(value) : null;
            ValueProp? props = Argument("props") is ValueProp property ? property : null;
            var dealer = Argument("dealer") as Creature;
            var source = Argument("cardSource") as CardModel;
            var invocation = new InvocationWave112(
                Interlocked.Increment(ref _invocationId),
                method.DeclaringType?.FullName + "." + method.Name,
                method.Name,
                amount,
                props,
                dealer,
                source,
                targets);
            Emit(invocation, stage, targets.Select(CreatureState).ToArray());
            return invocation;
        }
        catch (Exception exception)
        {
            Gap($"wave112 {method.DeclaringType?.Name}.{method.Name}", exception);
            return null;
        }
    }

    private static void Complete(InvocationWave112 invocation, Task? task)
    {
        try
        {
            var stage = task is null ? "completed" : task.IsCanceled ? "canceled" :
                task.IsFaulted ? "faulted" : "completed";
            Emit(invocation, stage, invocation.Targets.Select(CreatureState).ToArray(),
                task?.Exception?.GetBaseException().GetType().Name);
        }
        catch (Exception exception) { Gap($"wave112 completion {invocation.Operation}", exception); }
    }

    private static void Emit(
        InvocationWave112 invocation,
        string stage,
        IReadOnlyList<OracleCreatureWave112> targets,
        string? error = null)
    {
        var combat = CombatManager.Instance.IsInProgress
            ? CombatManager.Instance.DebugOnlyGetState() : null;
        var player = combat?.Players.SingleOrDefault();
        _write!(new(
            Interlocked.Increment(ref _eventOrder),
            invocation.Id,
            stage,
            invocation.Operation,
            invocation.RuntimeMethod,
            combat?.RoundNumber,
            combat?.CurrentSide.ToString(),
            player?.PlayerCombatState?.TurnNumber,
            Calendars(combat),
            invocation.Amount,
            invocation.Props is { } props ? Convert.ToInt32(props) : null,
            invocation.Props?.ToString(),
            invocation.Dealer is null ? null : CreatureId(invocation.Dealer),
            invocation.Dealer?.ModelId.ToString(),
            invocation.Source?.Id.ToString(),
            targets,
            error));
    }

    private static OracleCalendarWave112[] Calendars(CombatState? combat) => combat is null ? [] :
        combat.Players.SelectMany(player => player.Relics.OfType<StoneCalendar>().Select(calendar =>
            new OracleCalendarWave112(
                CreatureId(player.Creature),
                calendar.DisplayAmount,
                calendar.ShowCounter,
                (bool)(calendar.GetType().GetProperty("IsActivating",
                    BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)?.GetValue(calendar)
                    ?? throw new MissingMemberException(calendar.GetType().FullName, "IsActivating")),
                calendar.Status.ToString()))).ToArray();

    private static Creature[] Targets(object? instance, object? value) => value switch
    {
        Creature creature => [creature],
        IEnumerable enumerable => enumerable.Cast<object>().OfType<Creature>().ToArray(),
        _ when instance is Creature creature => [creature],
        _ => []
    };

    private static OracleCreatureWave112 CreatureState(Creature creature) => new(
        CreatureId(creature),
        creature.ModelId.ToString(),
        creature.CurrentHp,
        creature.MaxHp,
        creature.Block,
        creature.CombatState?.ContainsCreature(creature) == true);

    private static string CreatureId(Creature creature) =>
        creature.CombatId is { } id ? $"creature:{id}" : "creature:unassigned";

    private static void Gap(string context, Exception exception)
    {
        try { _gap?.Invoke(context, exception); }
        catch (Exception safetyFailure)
        {
            Console.Error.WriteLine($"[Spirefysh] Wave112 safety failure: {safetyFailure}");
        }
    }

    private sealed record InvocationWave112(
        long Id,
        string RuntimeMethod,
        string Operation,
        decimal? Amount,
        ValueProp? Props,
        Creature? Dealer,
        CardModel? Source,
        IReadOnlyList<Creature> Targets);
}
