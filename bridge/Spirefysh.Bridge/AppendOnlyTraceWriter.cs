using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Microsoft.Win32.SafeHandles;

namespace Spirefysh.Bridge;

/// <summary>Durable, append-only NDJSON storage. Existing bytes are never truncated or rewritten.</summary>
internal sealed class AppendOnlyTraceWriter : IDisposable
{
    private const int OpenReadWrite = 0x2;
    private const int OpenAppend = 0x8;
    private const int OpenNoFollow = 0x100;
    private const int OpenCloseOnExec = 0x01000000;
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

        FileStream stream;
        try
        {
            stream = new FileStream(path, FileMode.CreateNew, FileAccess.ReadWrite, FileShare.Read,
                4096, FileOptions.WriteThrough);
        }
        catch (IOException)
        {
            var descriptor = open(path, OpenReadWrite | OpenAppend | OpenNoFollow | OpenCloseOnExec);
            if (descriptor < 0)
                throw new IOException(
                    $"Could not open existing trace without following links (errno {Marshal.GetLastPInvokeError()}).");
            stream = new FileStream(new SafeFileHandle((IntPtr)descriptor, ownsHandle: true),
                FileAccess.ReadWrite, 4096);
        }
        try
        {
            var lastSequence = ValidateExisting(stream);
            stream.Seek(0, SeekOrigin.End);
            return new AppendOnlyTraceWriter(stream, lastSequence);
        }
        catch
        {
            stream.Dispose();
            throw;
        }
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

    private static long ValidateExisting(FileStream stream)
    {
        if (stream.Length == 0) return 0;
        stream.Seek(-1, SeekOrigin.End);
        if (stream.ReadByte() != (byte)'\n')
            throw new InvalidDataException(
                "Existing trace ends in a partial event; it is preserved and must be quarantined before restart.");
        stream.Seek(0, SeekOrigin.Begin);
        long previous = 0;
        var lineNumber = 0;
        using var reader = new StreamReader(stream, Utf8, false, 4096, leaveOpen: true);
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

    [DllImport("libSystem", SetLastError = true)]
    private static extern int open(string path, int flags);

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
