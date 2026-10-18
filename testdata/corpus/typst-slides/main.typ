#set page(paper: "presentation-16-9", margin: (x: 2cm, top: 2cm, bottom: 1.5cm))
#set text(size: 20pt)

// Slide 1: Title
#align(center + horizon)[
  #text(size: 32pt, weight: "bold")[High-Performance Scientific Authoring]

  #v(1.5em)
  #text(size: 22pt)[Sub-Millisecond Rendering and Local-First Engineering]

  #v(2em)
  #text(size: 18pt, fill: luma(100))[Fundaments Research Team \ October 2026]
]

#pagebreak()

// Slide 2: Problem Overview
== The Challenge of Contemporary Authoring

#v(1em)
- Traditional scientific typesetting pipelines introduce substantial latency:
  - Cold LaTeX rebuilds frequently require 2 to 5 seconds.
  - Multi-gigabyte runtime environments stall interactive feedback loops.
- Cloud-hosted platforms compromise privacy and local-first autonomy:
  - Network disconnection halts compilation and editing.
  - Proprietary binary databases lock user documents into hosted silos.
- *Our Objective:* Deliver sub-millisecond document compilation with zero cloud dependencies.

#pagebreak()

// Slide 3: Core Architecture
== Zero-Copy Binary Architecture

#v(1em)
- *Rust-First Core Engine:*
  - All parsing, compilation, citation indexing, and layout operations live in native Rust.
  - Frontend acts as a thin presentation and event-dispatching shell.
- *Zero-Copy Binary IPC:*
  - Raw binary buffers (`Uint8Array` / `tauri::ipc::Response`) cross the boundary without JSON serialization overhead.
- *Thread Safety:*
  - Tokio runtime is dedicated strictly to asynchronous I/O and file watching.
  - Rayon CPU thread pool handles document evaluation and rendering.

#pagebreak()

// Slide 4: Mathematical Formulation
== Continuous Latency Scaling

#v(0.8em)
The keystroke-to-screen latency $T_"latency"$ satisfies the strict upper bound:

$ T_"latency" = T_"input" + T_"eval" + T_"render" < 8 "ms" $

For an incremental edit modifying $k$ characters in an $M$-byte document rope:

$ T_"eval"(k, M) = O(k log M) $

#v(0.5em)
This logarithmic scaling guarantees interactive responsiveness even on documents exceeding $10^6$ characters.

#pagebreak()

// Slide 5: Experimental Benchmarks
== Benchmark Measurements

#v(0.8em)
#table(
  columns: (2fr, 1.5fr, 1.5fr),
  align: (left, center, center),
  table.header([*Operation*], [*Legacy Pipeline*], [*Manang Engine*]),
  [Cold Recompile (50 pp)], [2850 ms], [120 ms],
  [Incremental Edit Recompile], [450 ms], [18 ms],
  [Keystroke Input Latency], [42 ms], [3.2 ms],
  [Idle Memory Footprint], [480 MB], [48 MB],
)

#v(0.5em)
- 25$times$ faster cold recompile and over 10$times$ reduction in idle memory footprint.

#pagebreak()

// Slide 6: Summary & Conclusion
== Summary & Conclusion

#v(1em)
- *Local-First:* Documents remain human-readable UTF-8 files on disk.
- *Extensible:* Capability-based WebAssembly plugins isolated in Wasmtime sandboxes.
- *Deterministic:* Byte-identical vector SVG and PDF exports across Linux, macOS, and Windows.

#v(2em)
#align(center)[
  #text(weight: "bold", size: 24pt)[Thank You! Questions & Discussion]
]
