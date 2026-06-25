// Unity binding for the opcusdb swarm — drop into a Unity project and attach to a
// GameObject that has a MeshFilter + MeshRenderer (use a point/vertex-color
// material). Place the native library in Assets/Plugins/ (libopcusdb_ffi.dylib on
// macOS, opcusdb_ffi.dll on Windows, libopcusdb_ffi.so on Linux).
//
// The simulation runs in the Rust core via P/Invoke; Unity only renders the
// positions it reads from the library buffer each frame.

using System;
using System.Runtime.InteropServices;
using UnityEngine;

[RequireComponent(typeof(MeshFilter))]
public class OpcusdbSwarm : MonoBehaviour
{
    const string LIB = "opcusdb_ffi";

    [DllImport(LIB)] static extern IntPtr swarm_new(uint n, uint seed);
    [DllImport(LIB)] static extern void swarm_free(IntPtr h);
    [DllImport(LIB)] static extern void swarm_step(IntPtr h);
    [DllImport(LIB)] static extern uint swarm_len(IntPtr h);
    [DllImport(LIB)] static extern IntPtr swarm_positions_ptr(IntPtr h);
    [DllImport(LIB)] static extern int field_width();
    [DllImport(LIB)] static extern int field_height();

    public int count = 4000;
    public uint seed = 7;
    public float worldScale = 0.01f; // pixels -> world units (1000px -> 10 units)

    IntPtr handle;
    Mesh mesh;
    int[] buf;
    Vector3[] verts;
    int[] indices;

    void Start()
    {
        handle = swarm_new((uint)count, seed);
        int n = (int)swarm_len(handle);
        buf = new int[n * 2];
        verts = new Vector3[n];
        indices = new int[n];
        for (int i = 0; i < n; i++) indices[i] = i;

        mesh = new Mesh { indexFormat = UnityEngine.Rendering.IndexFormat.UInt32 };
        GetComponent<MeshFilter>().mesh = mesh;
    }

    void Update()
    {
        if (handle == IntPtr.Zero) return;
        swarm_step(handle);

        int n = (int)swarm_len(handle);
        Marshal.Copy(swarm_positions_ptr(handle), buf, 0, n * 2);
        for (int i = 0; i < n; i++)
            verts[i] = new Vector3(buf[i * 2] * worldScale, -buf[i * 2 + 1] * worldScale, 0f);

        mesh.Clear();
        mesh.vertices = verts;
        mesh.SetIndices(indices, MeshTopology.Points, 0); // render as a point cloud
    }

    void OnDestroy()
    {
        if (handle != IntPtr.Zero) { swarm_free(handle); handle = IntPtr.Zero; }
    }
}
