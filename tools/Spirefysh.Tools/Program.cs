using System.Collections;
using System.Diagnostics;
using System.Reflection;
using System.Runtime.Loader;
using System.Security.Cryptography;
using System.Text.Json;

record ProfileEntry(string Path, string Kind, long Size, string Sha256);
record ProfileSnapshot(int Schema, string Root, List<ProfileEntry> Entries);

sealed class GameLoadContext(string directory) : AssemblyLoadContext(true)
{
    protected override Assembly? Load(AssemblyName name)
    {
        if (name.Name?.StartsWith("System.") == true) return null;
        var path = Path.Combine(directory, $"{name.Name}.dll");
        return File.Exists(path) ? LoadFromAssemblyPath(path) : null;
    }
}

static class Program
{
    static int Main(string[] args)
    {
        try
        {
            return args switch
            {
                ["profile", "snapshot", var profile, var output] => Snapshot(profile, output),
                ["profile", "verify", var profile, var snapshot] => Verify(profile, snapshot),
                ["bridge", var game, var bridge] => VerifyBridge(game, bridge),
                ["advisor", var game, var bridge] => VerifyAdvisor(game, bridge),
                _ => throw new ArgumentException(
                    "usage: Spirefysh.Tools profile snapshot|verify PROFILE SNAPSHOT | bridge|advisor GAME_DLL BRIDGE_DLL")
            };
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception.Message);
            return 1;
        }
    }

    static int Snapshot(string profile, string output)
    {
        var snapshot = ReadProfile(profile);
        using var stream = new FileStream(Path.GetFullPath(output), FileMode.CreateNew);
        JsonSerializer.Serialize(stream, snapshot, new JsonSerializerOptions { WriteIndented = true });
        Console.WriteLine($"profile-snapshot valid=true entries={snapshot.Entries.Count}");
        return 0;
    }

    static int Verify(string profile, string snapshotPath)
    {
        using var stream = File.OpenRead(Path.GetFullPath(snapshotPath));
        var expected = JsonSerializer.Deserialize<ProfileSnapshot>(stream)
            ?? throw new InvalidDataException("profile snapshot is empty");
        var actual = ReadProfile(profile);
        if (expected.Schema != actual.Schema || !expected.Entries.SequenceEqual(actual.Entries))
            throw new InvalidDataException("profile differs from its baseline snapshot");
        Console.WriteLine($"profile-verify valid=true entries={actual.Entries.Count}");
        return 0;
    }

    static ProfileSnapshot ReadProfile(string profile)
    {
        var root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(profile));
        if (!Directory.Exists(root) || IsLink(root))
            throw new InvalidDataException("profile must be a direct directory");
        var entries = new List<ProfileEntry>();
        Scan(root, root, entries);
        entries.Sort((left, right) => StringComparer.Ordinal.Compare(left.Path, right.Path));
        return new(1, root, entries);
    }

    static void Scan(string root, string directory, List<ProfileEntry> entries)
    {
        foreach (var path in Directory.EnumerateFileSystemEntries(directory))
        {
            if (IsLink(path)) throw new InvalidDataException($"profile contains a link: {path}");
            var relative = Path.GetRelativePath(root, path).Replace(Path.DirectorySeparatorChar, '/');
            if (Directory.Exists(path))
            {
                entries.Add(new(relative, "directory", 0, ""));
                Scan(root, path, entries);
            }
            else if (File.Exists(path))
            {
                using var stream = File.OpenRead(path);
                entries.Add(new(relative, "file", stream.Length,
                    Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant()));
            }
            else
            {
                throw new InvalidDataException($"profile contains an unsupported entry: {path}");
            }
        }
    }

    static bool IsLink(string path) =>
        (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0;

    static int VerifyBridge(string gamePath, string bridgePath)
    {
        gamePath = Path.GetFullPath(gamePath);
        bridgePath = Path.GetFullPath(bridgePath);
        var gameName = AssemblyName.GetAssemblyName(gamePath);
        var bridgeName = AssemblyName.GetAssemblyName(bridgePath);
        if (bridgeName.Name != "Spirefysh.Bridge")
            throw new InvalidDataException($"unexpected bridge assembly name: {bridgeName.Name}");

        var context = new GameLoadContext(Path.GetDirectoryName(gamePath)!);
        try
        {
            var bridge = context.LoadFromAssemblyPath(bridgePath);
            var references = bridge.GetReferencedAssemblies();
            if (!references.Any(reference => reference.Name == gameName.Name
                && reference.Version == gameName.Version))
                throw new InvalidDataException("bridge does not reference the pinned game assembly");
            if (!references.Any(reference => reference.Name == "0Harmony"))
                throw new InvalidDataException("bridge does not reference Harmony");
            var entry = bridge.GetType("Spirefysh.Bridge.BridgeEntry", true)!;
            var initialize = entry.GetMethod("Initialize", BindingFlags.Public | BindingFlags.Static);
            if (initialize is null || initialize.ReturnType != typeof(void)
                || initialize.GetParameters().Length != 0)
                throw new InvalidDataException("bridge initializer has an unexpected signature");
            if (!entry.CustomAttributes.Any(attribute =>
                    attribute.AttributeType.Name == "ModInitializerAttribute"
                    && attribute.ConstructorArguments.Any(argument =>
                        argument.Value is string value && value == "Initialize")))
                throw new InvalidDataException("bridge initializer attribute is missing");
        }
        finally
        {
            context.Unload();
        }
        Console.WriteLine("bridge-verify valid=true");
        return 0;
    }

    static int VerifyAdvisor(string gamePath, string bridgePath)
    {
        var context = new GameLoadContext(Path.GetDirectoryName(Path.GetFullPath(gamePath))!);
        var assembly = context.LoadFromAssemblyPath(Path.GetFullPath(bridgePath));
        var clientType = assembly.GetType("Spirefysh.Bridge.AdvisorClient", true)!;
        var configType = assembly.GetType("Spirefysh.Bridge.AdvisorConfiguration", true)!;
        var config = Activator.CreateInstance(configType, false, false, "", "")!;
        var script = "while IFS= read -r line; do case $line in slow) sleep .2; print '{\"win_probability\":0.1,\"delta\":0}' ;; hang) sleep 10 ;; hover) print '{\"win_probability\":0.8,\"delta\":0}' ;; *) print '{\"win_probability\":0.5,\"delta\":0}' ;; esac; done";
        var start = new ProcessStartInfo("/bin/zsh")
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            UseShellExecute = false
        };
        start.ArgumentList.Add("-c");
        start.ArgumentList.Add(script);
        using var process = Process.Start(start)!;
        var constructor = clientType.GetConstructors(BindingFlags.Instance | BindingFlags.NonPublic).Single();
        using var client = (IDisposable)constructor.Invoke([process, config]);
        var enqueue = clientType.GetMethod("Enqueue", BindingFlags.Instance | BindingFlags.NonPublic)!;
        object? Call(string key, string request, int priority) =>
            enqueue.Invoke(client, [key, request, priority]);
        object Wait(string key, string request, int priority, double seconds = 2)
        {
            var deadline = Stopwatch.GetTimestamp() + seconds * Stopwatch.Frequency;
            while (Stopwatch.GetTimestamp() < deadline)
            {
                var result = Call(key, request, priority);
                if (result is not null) return result;
                Thread.Sleep(5);
            }
            throw new TimeoutException($"advisor result timed out: {key}");
        }

        Call("slow", "slow", 0);
        Call("discarded-low", "discarded-low", 0);
        Call("hover", "hover", 1);
        Call("discarded-later-low", "discarded-later-low", 0);
        var hover = Wait("hover", "hover", 1);
        if ((double)hover.GetType().GetProperty("WinProbability")!.GetValue(hover)! != .8)
            throw new InvalidDataException("hover request did not win coalescing");
        var cache = (IDictionary)clientType.GetField(
            "_cache", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(client)!;
        if (!cache.Contains("slow") || !cache.Contains("hover") || cache.Contains("discarded-low") ||
            cache.Contains("discarded-later-low"))
            throw new InvalidDataException("advisor priorities were not coalesced");
        for (var index = 0; index < 260; index++)
            Wait($"cache-{index}", $"cache-{index}", 1);
        if (cache.Count != 256)
            throw new InvalidDataException($"advisor cache is not bounded: {cache.Count}");

        Call("hang", "hang", 1);
        var failed = false;
        var timeout = Stopwatch.GetTimestamp() + 7 * Stopwatch.Frequency;
        while (Stopwatch.GetTimestamp() < timeout && !failed)
        {
            try { Call("hang", "hang", 1); }
            catch (TargetInvocationException exception)
                when (exception.InnerException is InvalidOperationException) { failed = true; }
            Thread.Sleep(20);
        }
        if (!failed || !process.HasExited)
            throw new InvalidDataException("hung advisor was not failed and killed");
        Console.WriteLine("advisor-worker-verify valid=true cache=256 coalescing=true timeout=true");
        return 0;
    }
}
