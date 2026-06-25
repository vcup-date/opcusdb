// Godot 4 (.NET / C#) binding for the opcusdb swarm. Attach to a Node2D in a
// Godot Mono project; place the native library where Godot can load it (next to
// the project / in res:// with an export config, or a system lib path).
//
// The simulation runs in the Rust core via P/Invoke; Godot only draws the
// positions it reads from the library buffer each frame.
//
// (GDScript cannot P/Invoke directly — for a pure-GDScript binding wrap this same
// C-ABI in a GDExtension. The C-ABI is ready for that; this C# path is simplest.)

using Godot;
using System;
using System.Runtime.InteropServices;

public partial class OpcusdbSwarm : Node2D
{
    const string LIB = "opcusdb_ffi";

    [DllImport(LIB)] static extern IntPtr swarm_new(uint n, uint seed);
    [DllImport(LIB)] static extern void swarm_free(IntPtr h);
    [DllImport(LIB)] static extern void swarm_step(IntPtr h);
    [DllImport(LIB)] static extern uint swarm_len(IntPtr h);
    [DllImport(LIB)] static extern IntPtr swarm_positions_ptr(IntPtr h);

    [Export] public int Count = 4000;
    [Export] public uint Seed = 7;
    [Export] public float Scale = 0.6f; // pixels -> screen px

    IntPtr _handle;
    int _n;
    int[] _buf;

    public override void _Ready()
    {
        _handle = swarm_new((uint)Count, Seed);
        _n = (int)swarm_len(_handle);
        _buf = new int[_n * 2];
    }

    public override void _Process(double delta)
    {
        if (_handle == IntPtr.Zero) return;
        swarm_step(_handle);
        Marshal.Copy(swarm_positions_ptr(_handle), _buf, 0, _n * 2);
        QueueRedraw();
    }

    public override void _Draw()
    {
        var color = new Color(0.23f, 0.63f, 1.0f);
        for (int i = 0; i < _n; i++)
            DrawCircle(new Vector2(_buf[i * 2] * Scale, _buf[i * 2 + 1] * Scale), 1.5f, color);
    }

    public override void _ExitTree()
    {
        if (_handle != IntPtr.Zero) { swarm_free(_handle); _handle = IntPtr.Zero; }
    }
}
