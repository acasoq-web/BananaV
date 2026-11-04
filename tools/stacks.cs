// A small sampling profiler for Windows: which threads of a running process
// use the processor, and where. No debugger needed; the symbols come from the
// PDBs beside the executable.
//
//   csc -nologo -platform:x64 -out:stacks.exe tools\stacks.cs
//   stacks.exe <pid> <folder of the PDB> [seconds to measure] [threads] [stacks per thread]
//
// STACK_DUMP=<file> appends every stack seen, "count<TAB>thread<TAB>frame;frame;…".
//
// For full symbols, profile a copy built with them, in a folder of its own
// (no link-time optimization: a minute to rebuild after a change):
//
//   CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_PROFILE_RELEASE_LTO=off \
//   CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_TARGET_DIR=<a folder> \
//   cargo build --release -p bananav
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;

static class Stacks
{
    [DllImport("kernel32")] static extern IntPtr OpenThread(uint access, bool inherit, uint id);
    [DllImport("kernel32")] static extern uint SuspendThread(IntPtr thread);
    [DllImport("kernel32")] static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32")] static extern bool GetThreadContext(IntPtr thread, IntPtr context);
    [DllImport("kernel32")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32")] static extern IntPtr OpenProcess(uint access, bool inherit, uint id);
    [DllImport("kernel32", CharSet = CharSet.Unicode)] static extern IntPtr LoadLibrary(string name);
    [DllImport("kernel32", CharSet = CharSet.Ansi)] static extern IntPtr GetProcAddress(IntPtr module, string name);
    [DllImport("kernel32", CharSet = CharSet.Unicode)] static extern int GetThreadDescription(IntPtr thread, out IntPtr description);
    [DllImport("kernel32")] static extern IntPtr LocalFree(IntPtr memory);
    [DllImport("dbghelp", CharSet = CharSet.Unicode, EntryPoint = "SymInitializeW")] static extern bool SymInitialize(IntPtr process, string path, bool invade);
    [DllImport("dbghelp")] static extern uint SymSetOptions(uint options);
    [DllImport("dbghelp")] static extern bool StackWalk64(uint machine, IntPtr process, IntPtr thread, IntPtr frame, IntPtr context, IntPtr read, IntPtr tables, IntPtr bases, IntPtr translate);
    [DllImport("dbghelp")] static extern bool SymFromAddr(IntPtr process, ulong address, out ulong displacement, IntPtr symbol);

    const uint THREAD_ALL = 0x1FFFFF, THREAD_QUERY_LIMITED = 0x1000, PROCESS_ALL = 0x1FFFFF;
    // SYMOPT_UNDNAME, SYMOPT_DEFERRED_LOADS, SYMOPT_LOAD_LINES.
    const uint SYM_OPTIONS = 0x2 | 0x4 | 0x10;
    // IMAGE_FILE_MACHINE_AMD64.
    const uint MACHINE = 0x8664;
    // CONTEXT (x64): its size, ContextFlags, Rsp, Rbp, Rip; CONTEXT_FULL.
    const int CONTEXT_SIZE = 1232, CONTEXT_FLAGS = 0x30, CONTEXT_RSP = 0x98, CONTEXT_RBP = 0xA0, CONTEXT_RIP = 0xF8, CONTEXT_FULL = 0x10000B;
    // STACKFRAME64: AddrPC, AddrFrame, AddrStack (offset, then its mode: flat).
    const int FRAME_SIZE = 512, FRAME_PC = 0, FRAME_BASE = 32, FRAME_STACK = 48, ADDRESS_MODE = 12, MODE_FLAT = 3;
    // SYMBOL_INFO: its size, MaxNameLen, Name.
    const int SYMBOL_STRUCT = 88, SYMBOL_MAX_NAME = 80, SYMBOL_NAME = 84, NAME_ROOM = 400;

    static string ThreadName(uint id)
    {
        IntPtr thread = OpenThread(THREAD_QUERY_LIMITED, false, id);
        if (thread == IntPtr.Zero) return "";
        IntPtr text;
        string name = "";
        if (GetThreadDescription(thread, out text) >= 0 && text != IntPtr.Zero)
        {
            name = Marshal.PtrToStringUni(text);
            LocalFree(text);
        }
        CloseHandle(thread);
        return name;
    }

    static List<string> Stack(IntPtr process, uint id, IntPtr contextMemory, IntPtr frame, IntPtr symbol, IntPtr tables, IntPtr bases, ProcessModule[] modules)
    {
        var frames = new List<string>();
        IntPtr thread = OpenThread(THREAD_ALL, false, id);
        if (thread == IntPtr.Zero) return frames;
        if (SuspendThread(thread) == 0xFFFFFFFF)
        {
            CloseHandle(thread);
            return frames;
        }
        try
        {
            // CONTEXT aligned to 16 bytes.
            IntPtr context = new IntPtr((contextMemory.ToInt64() + 15) & ~15L);
            for (int i = 0; i < CONTEXT_SIZE; i++) Marshal.WriteByte(context, i, 0);
            Marshal.WriteInt32(context, CONTEXT_FLAGS, CONTEXT_FULL);
            if (!GetThreadContext(thread, context)) return frames;
            for (int i = 0; i < FRAME_SIZE; i++) Marshal.WriteByte(frame, i, 0);
            Marshal.WriteInt64(frame, FRAME_PC, Marshal.ReadInt64(context, CONTEXT_RIP));
            Marshal.WriteInt32(frame, FRAME_PC + ADDRESS_MODE, MODE_FLAT);
            Marshal.WriteInt64(frame, FRAME_BASE, Marshal.ReadInt64(context, CONTEXT_RBP));
            Marshal.WriteInt32(frame, FRAME_BASE + ADDRESS_MODE, MODE_FLAT);
            Marshal.WriteInt64(frame, FRAME_STACK, Marshal.ReadInt64(context, CONTEXT_RSP));
            Marshal.WriteInt32(frame, FRAME_STACK + ADDRESS_MODE, MODE_FLAT);
            for (int depth = 0; depth < 64; depth++)
            {
                if (!StackWalk64(MACHINE, process, thread, frame, context, IntPtr.Zero, tables, bases, IntPtr.Zero)) break;
                ulong pc = (ulong)Marshal.ReadInt64(frame, FRAME_PC);
                if (pc == 0) break;
                Marshal.WriteInt32(symbol, 0, SYMBOL_STRUCT);
                Marshal.WriteInt32(symbol, SYMBOL_MAX_NAME, NAME_ROOM);
                ulong displacement;
                string name = SymFromAddr(process, pc, out displacement, symbol)
                    ? Marshal.PtrToStringAnsi(new IntPtr(symbol.ToInt64() + SYMBOL_NAME))
                    : "?";
                string module = "?";
                foreach (var m in modules)
                {
                    long start = m.BaseAddress.ToInt64();
                    if ((long)pc >= start && (long)pc < start + m.ModuleMemorySize)
                    {
                        module = m.ModuleName;
                        break;
                    }
                }
                frames.Add(module + "!" + name);
            }
        }
        finally
        {
            ResumeThread(thread);
            CloseHandle(thread);
        }
        return frames;
    }

    static void Main(string[] args)
    {
        uint pid = uint.Parse(args[0]);
        string symbols = args[1];
        double seconds = args.Length > 2 ? double.Parse(args[2]) : 3;
        int top = args.Length > 3 ? int.Parse(args[3]) : 4;
        int samples = args.Length > 4 ? int.Parse(args[4]) : 40;
        var target = Process.GetProcessById((int)pid);
        IntPtr process = OpenProcess(PROCESS_ALL, false, pid);
        SymSetOptions(SYM_OPTIONS);
        if (!SymInitialize(process, symbols, true)) Console.WriteLine("SymInitialize failed");
        IntPtr help = LoadLibrary("dbghelp.dll");
        IntPtr tables = GetProcAddress(help, "SymFunctionTableAccess64"), bases = GetProcAddress(help, "SymGetModuleBase64");

        // Each thread's processor time over the seconds asked for.
        var before = new Dictionary<int, double>();
        target.Refresh();
        foreach (ProcessThread t in target.Threads)
        {
            try { before[t.Id] = t.TotalProcessorTime.TotalMilliseconds; } catch { }
        }
        var clock = Stopwatch.StartNew();
        Thread.Sleep((int)(seconds * 1000));
        target.Refresh();
        double elapsed = clock.Elapsed.TotalMilliseconds;
        var rows = new List<Tuple<int, double>>();
        foreach (ProcessThread t in target.Threads)
        {
            try
            {
                double b;
                before.TryGetValue(t.Id, out b);
                rows.Add(Tuple.Create(t.Id, (t.TotalProcessorTime.TotalMilliseconds - b) / elapsed));
            }
            catch { }
        }
        rows.Sort((x, y) => y.Item2.CompareTo(x.Item2));
        Console.WriteLine("threads {0}, total {1:F2} cores", rows.Count, rows.Sum(r => r.Item2));
        foreach (var r in rows.Take(Math.Max(top, 12)))
            Console.WriteLine("  {0,6} {1,6:F3} {2}", r.Item1, r.Item2, ThreadName((uint)r.Item1));

        // The busiest threads' stacks.
        var modules = target.Modules.Cast<ProcessModule>().ToArray();
        IntPtr contextMemory = Marshal.AllocHGlobal(CONTEXT_SIZE + 16), frame = Marshal.AllocHGlobal(FRAME_SIZE), symbol = Marshal.AllocHGlobal(SYMBOL_NAME + NAME_ROOM + 1);
        string dump = Environment.GetEnvironmentVariable("STACK_DUMP");
        foreach (var r in rows.Take(top))
        {
            if (r.Item2 < 0.02) continue;
            Console.WriteLine();
            Console.WriteLine("=== thread {0} [{1}] {2:F3} cores", r.Item1, ThreadName((uint)r.Item1), r.Item2);
            var topFrames = new Dictionary<string, int>();
            var everywhere = new Dictionary<string, int>();
            var stacks = new Dictionary<string, int>();
            for (int k = 0; k < samples; k++)
            {
                var stack = Stack(process, (uint)r.Item1, contextMemory, frame, symbol, tables, bases, modules);
                if (stack.Count > 0)
                {
                    int c;
                    topFrames.TryGetValue(stack[0], out c);
                    topFrames[stack[0]] = c + 1;
                    foreach (var f in stack.Distinct())
                    {
                        everywhere.TryGetValue(f, out c);
                        everywhere[f] = c + 1;
                    }
                    string key = string.Join(";", stack);
                    stacks.TryGetValue(key, out c);
                    stacks[key] = c + 1;
                }
                Thread.Sleep(25);
            }
            Console.WriteLine("  top frames:");
            foreach (var kv in topFrames.OrderByDescending(x => x.Value).Take(10))
                Console.WriteLine("    {0,3} {1}", kv.Value, kv.Key);
            Console.WriteLine("  on the stack:");
            foreach (var kv in everywhere.OrderByDescending(x => x.Value).Take(24))
                Console.WriteLine("    {0,3} {1}", kv.Value, kv.Key);
            if (!string.IsNullOrEmpty(dump))
                System.IO.File.AppendAllLines(dump, stacks.Select(kv => kv.Value + "\t" + r.Item1 + "\t" + kv.Key));
        }
    }
}
