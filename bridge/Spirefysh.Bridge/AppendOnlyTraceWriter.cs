using System;
using System.IO;
using System.Text;
using System.Text.Json;

namespace Spirefysh.Bridge;

/// <summary>Durable, append-only NDJSON storage. Existing bytes are never truncated or rewritten.</summary>
internal sealed class AppendOnlyTraceWriter : IDisposable
{
    private static readonly UTF8Encoding Utf8 = new(encoderShouldEmitUTF8Identifier: false);
    private readonly object _gate = new();
    private readonly FileStream _stream;

    private AppendOnlyTraceWriter(FileStream stream, long lastSequence)
    {
        _stream = stream;
        LastSequence = lastSequence;
    }

    public long LastSequence { get; private set; }

    public static AppendOnlyTraceWriter Open(string path)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(path);
        if (!Path.IsPathFullyQualified(path))
            throw new InvalidOperationException("Trace path must be absolute.");
        path = Path.GetFullPath(path);
        if (!Path.GetExtension(path).Equals(".ndjson", StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Trace path must end in .ndjson.");
        var parent = Path.GetDirectoryName(path)
            ?? throw new InvalidOperationException("Trace path has no parent directory.");
        if (!Directory.Exists(parent))
            throw new DirectoryNotFoundException($"Trace parent directory does not exist: {parent}");
        RefuseSymlinkPath(parent);

        var lastSequence = ValidateExisting(path);
        var stream = new FileStream(
            path,
            FileMode.OpenOrCreate,
            FileAccess.Write,
            FileShare.Read,
            bufferSize: 4096,
            FileOptions.WriteThrough);
        stream.Seek(0, SeekOrigin.End);
        return new AppendOnlyTraceWriter(stream, lastSequence);
    }

    public long Write<T>(T value, Func<T, long, T> withSequence)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentNullException.ThrowIfNull(withSequence);
        lock (_gate)
        {
            var sequence = checked(LastSequence + 1);
            var payload = JsonSerializer.SerializeToUtf8Bytes(
                withSequence(value, sequence),
                TraceJson.Options);
            _stream.Write(payload);
            _stream.WriteByte((byte)'\n');
            _stream.Flush(flushToDisk: true);
            LastSequence = sequence;
            return sequence;
        }
    }

    public void Dispose()
    {
        lock (_gate) _stream.Dispose();
    }

    private static long ValidateExisting(string path)
    {
        if (!File.Exists(path)) return 0;
        var bytes = File.ReadAllBytes(path);
        if (bytes.Length == 0) return 0;
        if (bytes[^1] != (byte)'\n')
            throw new InvalidDataException(
                "Existing trace ends in a partial event; it is preserved and must be quarantined before restart.");
        long previous = 0;
        var lineNumber = 0;
        using var reader = new StringReader(Utf8.GetString(bytes));
        string? line;
        while ((line = reader.ReadLine()) is not null)
        {
            lineNumber++;
            if (line.Length == 0)
                throw new InvalidDataException($"Existing trace contains a blank line at {lineNumber}.");
            using var document = JsonDocument.Parse(line, new JsonDocumentOptions
            {
                AllowTrailingCommas = false,
                CommentHandling = JsonCommentHandling.Disallow,
                MaxDepth = 128
            });
            if (document.RootElement.ValueKind != JsonValueKind.Object ||
                !document.RootElement.TryGetProperty("sequence", out var sequenceElement) ||
                !sequenceElement.TryGetInt64(out var sequence) ||
                sequence <= previous)
                throw new InvalidDataException(
                    $"Existing trace sequence is missing or nonmonotonic at line {lineNumber}.");
            previous = sequence;
        }
        return previous;
    }

    private static void RefuseSymlinkPath(string path)
    {
        var current = new DirectoryInfo(Path.GetFullPath(path));
        while (current is not null)
        {
            if ((current.Attributes & FileAttributes.ReparsePoint) != 0)
                throw new InvalidOperationException(
                    $"Trace path may not traverse a symbolic link: {current.FullName}");
            current = current.Parent;
        }
    }
}

internal static class TraceJson
{
    internal static JsonSerializerOptions Options { get; } = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower
    };
}
