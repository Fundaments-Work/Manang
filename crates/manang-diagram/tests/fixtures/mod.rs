//! One small fixture per supported diagram family, shared by the snapshot
//! and property tests.

/// `(name, language, source)`.
pub const FIXTURES: &[(&str, &str, &str)] = &[
    (
        "flowchart",
        "mermaid",
        "flowchart LR\n  A[Draft] --> B(Review)\n  B -->|accepted| C[Publish]:::accent\n  B -.->|changes| A\n  subgraph Build\n    C --> D{{Typst}}\n  end\n",
    ),
    (
        "sequence",
        "mermaid",
        "sequenceDiagram\n  participant E as Editor\n  participant K as Kernel\n  E->>K: invoke(diagram.render)\n  activate K\n  K-->>E: SVG bytes\n  deactivate K\n  Note over E,K: Zero-copy binary IPC\n",
    ),
    (
        "class",
        "mermaid",
        "classDiagram\n  class Plugin {\n    +invoke(command, input) Payload\n  }\n  class DiagramPlugin\n  Plugin <|-- DiagramPlugin\n  DiagramPlugin *-- DiagramEngine : engines\n",
    ),
    (
        "state",
        "mermaid",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Rendering : edit\n  Rendering --> Idle : done\n  Rendering --> Failed : parse error\n  Failed --> Idle : fix\n",
    ),
    (
        "er",
        "mermaid",
        "erDiagram\n  DOCUMENT ||--o{ DIAGRAM : contains\n  DIAGRAM {\n    string language\n    string source\n  }\n",
    ),
    (
        "gantt",
        "mermaid",
        "gantt\n  title Release\n  dateFormat YYYY-MM-DD\n  section Core\n  Diagram crate :a1, 2026-10-01, 7d\n  Frontend      :after a1, 5d\n",
    ),
    (
        "pie",
        "mermaid",
        "pie title Time spent\n  \"Writing\" : 60\n  \"Figures\" : 25\n  \"References\" : 15\n",
    ),
    (
        "mindmap",
        "mermaid",
        "mindmap\n  root((Manang))\n    Typst\n    LaTeX\n    Markdown\n      Diagrams\n",
    ),
    (
        "timeline",
        "mermaid",
        "timeline\n  title Milestones\n  2026-09 : Kernel\n  2026-10 : Diagrams : Writing tools\n",
    ),
    (
        "journey",
        "mermaid",
        "journey\n  title Writing a paper\n  section Draft\n    Outline: 5: Author\n    Figures: 3: Author\n",
    ),
    (
        "gitgraph",
        "mermaid",
        "gitGraph\n  commit\n  branch agent/diagram\n  commit\n  commit\n  checkout main\n  merge agent/diagram\n",
    ),
    (
        "quadrant",
        "mermaid",
        "quadrantChart\n  title Effort and impact\n  x-axis Low effort --> High effort\n  y-axis Low impact --> High impact\n  Diagrams: [0.3, 0.8]\n  Themes: [0.6, 0.4]\n",
    ),
    (
        "xychart",
        "mermaid",
        "xychart-beta\n  title \"Render time\"\n  x-axis [a, b, c]\n  y-axis \"ms\" 0 --> 20\n  bar [4, 9, 13]\n  line [4, 9, 13]\n",
    ),
    (
        "sankey",
        "mermaid",
        "sankey-beta\nSource,Parse,10\nParse,Layout,8\nParse,Errors,2\n",
    ),
    (
        "block",
        "mermaid",
        "block-beta\n  columns 3\n  a[\"Source\"] b[\"Engine\"] c[\"SVG\"]\n  a --> b\n  b --> c\n",
    ),
    (
        "packet",
        "mermaid",
        "packet-beta\n  0-15: \"Source port\"\n  16-31: \"Destination port\"\n",
    ),
    (
        "requirement",
        "mermaid",
        "requirementDiagram\n  requirement determinism {\n    id: 1\n    text: same input, same bytes\n    risk: low\n    verifymethod: test\n  }\n  element snapshots {\n    type: test\n  }\n  snapshots - verifies -> determinism\n",
    ),
    (
        "c4",
        "mermaid",
        "C4Context\n  title Diagram rendering\n  Person(author, \"Author\")\n  System(manang, \"Manang\")\n  Rel(author, manang, \"writes\")\n",
    ),
    (
        "architecture",
        "mermaid",
        "architecture-beta\n  group app(cloud)[App]\n  service kernel(server)[Kernel] in app\n  service disk(disk)[Cache] in app\n  kernel:R --> L:disk\n",
    ),
    (
        "kanban",
        "mermaid",
        "kanban\n  todo[Todo]\n    t1[Write ADR]\n  done[Done]\n    t2[Pin merman]\n",
    ),
    (
        "radar",
        "mermaid",
        "radar-beta\n  axis a[\"Speed\"], b[\"Fidelity\"], c[\"Size\"]\n  curve x[\"merman\"]{4, 3, 4}\n",
    ),
    (
        "pikchr",
        "pikchr",
        "box \"Source\" fit\narrow\nbox \"Engine\" fit\narrow\ncircle \"SVG\" fit\n",
    ),
    (
        "svgbob",
        "svgbob",
        "+--------+     .------.\n| Source |---->| SVG  |\n+--------+     '------'\n     |\n     v\n   (cache)\n",
    ),
];
