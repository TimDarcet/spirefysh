using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.Versioning;
using System.Security.Cryptography;
using HarmonyLib;
using MegaCrit.Sts2.Core.Combat;
using MegaCrit.Sts2.Core.GameActions;
using MegaCrit.Sts2.Core.Helpers;
using MegaCrit.Sts2.Core.Modding;
using MegaCrit.Sts2.Core.Nodes;
using MegaCrit.Sts2.Core.Rooms;
using MegaCrit.Sts2.Core.Runs;

[assembly: TargetFramework(".NETCoreApp,Version=v9.0", FrameworkDisplayName = ".NET 9.0")]
[assembly: AssemblyTitle("Spirefysh.Bridge")]
[assembly: AssemblyVersion("0.1.0.0")]
[assembly: AssemblyInformationalVersion("0.1.0+59260271")]

namespace Spirefysh.Bridge;

[ModInitializer(nameof(Initialize))]
public static class BridgeEntry
{
    internal const string ModId = "spirefysh.bridge";
    internal const string SupportedRelease = "0.107.1";
    internal const string SupportedCommit = "59260271157f76a2896f0eab5bc6ea1245d8b314";
    internal const string SupportedAssemblySha256 = "e7ceb80669bfaf5c8fccabaa126ae2bb283aba514be5b5b55612579cfd285f18";
    internal const string SupportedContentArchiveSha256 = "62c887be791250b7a90c6cd929c19d03d33e17cc76aa7ff610b2889cccdadadb";
    internal const string SupportedReleaseInfoSha256 = "93838093ff803a60a8f086355a1d1a9cb103358089f8a46ae41743ddd8919b42";
    private static AdvisorClient? _advisor;
    private static TraceCapture? _capture;
    private static bool _modsVerified;
    private static bool _runStartedAttached;
    private static bool _actionsAttached;
    private static bool _checksumAttached;
    private static bool _combatAttached;

    public static void Initialize()
    {
        try
        {
            VerifyGameAssembly();
            _capture = TraceCapture.CreateFromConfiguration();
            _advisor = AdvisorClient.CreateFromConfiguration();
            if (_capture is null && _advisor is null)
            {
                Console.Error.WriteLine("[Spirefysh] No trace or advisor configuration was found; bridge is disabled.");
                return;
            }

            var harmony = new Harmony(ModId);
            if (_capture is not null)
            {
                harmony.Patch(
                    RequiredMethod(typeof(NGame), nameof(NGame.IsReleaseGame)),
                    postfix: new HarmonyMethod(typeof(BridgeEntry), nameof(AllowAutoSlay)));
                harmony.Patch(
                    RequiredMethod(typeof(CommandLineHelper), nameof(CommandLineHelper.HasArg)),
                    postfix: new HarmonyMethod(typeof(BridgeEntry), nameof(ForceAutoSlay)));
                harmony.Patch(
                    RequiredMethod(typeof(CommandLineHelper), nameof(CommandLineHelper.GetValue)),
                    postfix: new HarmonyMethod(typeof(BridgeEntry), nameof(AutoSlayValue)));
                harmony.Patch(
                    RequiredMethod(typeof(RunManager), nameof(RunManager.Launch)),
                    prefix: new HarmonyMethod(typeof(BridgeEntry), nameof(BeforeRunManagerLaunch)));
                harmony.Patch(
                    RequiredMethod(typeof(RunManager), nameof(RunManager.SetUpNewSingleplayer)),
                    prefix: new HarmonyMethod(typeof(BridgeEntry), nameof(ConfigureNewSingleplayer)));
                foreach (var methodName in new[]
                         {
                             nameof(RunManager.SetUpNewMultiplayer),
                             nameof(RunManager.SetUpSavedSingleplayer),
                             nameof(RunManager.SetUpSavedMultiplayer),
                             nameof(RunManager.SetUpReplay),
                             nameof(RunManager.SetUpTest)
                         })
                    harmony.Patch(
                        RequiredMethod(typeof(RunManager), methodName),
                        prefix: new HarmonyMethod(typeof(BridgeEntry), nameof(RejectUnsupportedRunSetup)));
                BridgePatches.Install(harmony, _capture);
                CommandPhaseTelemetryWave112.Install(
                    harmony,
                    _capture.RecordOracleCommandWave112,
                    _capture.CaptureGap,
                    () => _capture.Enabled);
            }
            if (_advisor?.ShowWinProbabilityDelta == true) AdvisorHover.Install(harmony, _advisor);
            if (_advisor?.ShowCurrentWinProbability == true) AdvisorTopBar.Install(_advisor);
            AppDomain.CurrentDomain.ProcessExit += (_, _) =>
            {
                _capture?.RecordIncompleteShutdown();
                _advisor?.Dispose();
            };
            Console.WriteLine($"[Spirefysh] Bridge initialized; telemetry={_capture is not null} advisor={_advisor is not null}.");
        }
        catch (Exception exception)
        {
            _capture?.Disable($"initialization failed: {exception.GetType().Name}: {exception.Message}");
            Console.Error.WriteLine($"[Spirefysh] Refusing telemetry: {exception}");
        }
    }

    private static void VerifyGameAssembly()
    {
        var assembly = typeof(RunManager).Assembly;
        var version = assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion;
        if (version is null || !version.EndsWith(SupportedCommit, StringComparison.Ordinal))
            throw new InvalidOperationException($"unsupported STS2 commit '{version ?? "<missing>"}'");

        if (!OperatingSystem.IsMacOS())
            throw new PlatformNotSupportedException("the pinned bridge supports only the verified macOS arm64 runtime");

        using var stream = File.OpenRead(assembly.Location);
        var hash = Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();
        if (hash != SupportedAssemblySha256)
            throw new InvalidOperationException($"unsupported STS2 assembly SHA-256 '{hash}'");

        var resourcesDirectory = Directory.GetParent(Path.GetDirectoryName(assembly.Location)!)?.FullName
            ?? throw new InvalidOperationException("STS2 Resources directory is unavailable.");
        VerifyFileHash(
            Path.Combine(resourcesDirectory, "Slay the Spire 2.pck"),
            SupportedContentArchiveSha256,
            "content archive");
        VerifyFileHash(
            Path.Combine(resourcesDirectory, "release_info.json"),
            SupportedReleaseInfoSha256,
            "release metadata");
    }

    private static void VerifyFileHash(string path, string expected, string label)
    {
        using var stream = File.OpenRead(path);
        var actual = Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();
        if (actual != expected)
            throw new InvalidOperationException($"unsupported STS2 {label} SHA-256 '{actual}'");
    }

    private static MethodInfo RequiredMethod(Type type, string methodName) =>
        AccessTools.Method(type, methodName)
        ?? throw new MissingMethodException(type.FullName, methodName);

    private static void AllowAutoSlay(ref bool __result) => __result = false;

    private static void ForceAutoSlay(string key, ref bool __result)
    {
        if (key == "autoslay") __result = true;
    }

    private static void AutoSlayValue(string key, ref string? __result)
    {
        if (key == "seed" && string.IsNullOrWhiteSpace(__result))
            __result = _capture?.AutoSlaySeed;
    }

    // Harmony binds these by argument name. New oracle runs never create or update a run save.
    private static void ConfigureNewSingleplayer(
        RunManager __instance,
        ref bool shouldSave,
        DateTimeOffset? dailyTime)
    {
        Attach(__instance);
        shouldSave = false;
        if (dailyTime is not null)
            _capture?.CaptureGap("run setup", "daily runs are outside the supported oracle scope");
    }

    private static void RejectUnsupportedRunSetup(MethodBase __originalMethod) =>
        _capture?.CaptureGap("run setup", $"unsupported entry point: {__originalMethod.Name}");

    // Attach before Launch so the public RunStarted event cannot be missed inside Launch.
    private static void BeforeRunManagerLaunch(RunManager __instance) => Attach(__instance);

    private static void Attach(RunManager runManager)
    {
        var capture = _capture;
        if (capture is null || !capture.Enabled) return;
        try
        {
            if (!_modsVerified)
            {
                var loaded = ModManager.GetLoadedMods().ToArray();
                if (loaded.Length != 1 || loaded[0].manifest is not { } manifest ||
                    manifest.id != ModId || !manifest.affectsGameplay)
                {
                    capture.Disable($"unsupported loaded mods: {string.Join(',', loaded.Select(x => x.manifest?.id ?? "<missing>"))}");
                    return;
                }
                _modsVerified = true;
            }

            if (!_runStartedAttached)
            {
                runManager.RunStarted += OnRunStarted;
                _runStartedAttached = true;
                capture.RecordLifecycle("bridge_attached", captureSnapshot: false);
            }
            if (!_actionsAttached && runManager.ActionQueueSet is { } actions)
            {
                actions.ActionEnqueued += capture.OnActionEnqueued;
                _actionsAttached = true;
            }
            if (!_checksumAttached && runManager.ChecksumTracker is { } checksums)
            {
                checksums.ChecksumGenerated += capture.OnChecksumGenerated;
                _checksumAttached = true;
            }

            AttachCombat();
        }
        catch (Exception exception)
        {
            capture.Disable($"attachment failed: {exception.GetType().Name}: {exception.Message}");
        }
    }

    private static void OnRunStarted(RunState run)
    {
        Attach(RunManager.Instance);
        AttachCombat();
        _capture?.OnRunStarted(run);
    }

    private static void AttachCombat()
    {
        if (_combatAttached || CombatManager.Instance is not { } combat || _capture is not { } capture)
            return;
        combat.CombatSetUp += capture.OnCombatSetUp;
        combat.CombatEnded += capture.OnCombatEnded;
        _combatAttached = true;
    }
}
