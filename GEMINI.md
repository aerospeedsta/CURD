# GEMINI Instructions

Before making any edits in this repository:

1. Read `AGENTS_QUICKSTART.md`.
2. Then read `AGENTS.md` fully.
3. Follow `AGENTS.md` as the primary repository contract for behavior, safety, mode gating, and API invariants.

If there is any conflict:
- explicit user request in current session wins,
- otherwise follow `AGENTS.md`.

Do not proceed with code changes until steps 1-2 are completed.

## Design Context (CURD-Wiki GUI)

### Users & Purpose
- **Users**: Software engineers, architects, and technical reviewers managing complex, large-scale codebases.
- **Context**: Deep technical work, code review, architectural planning, and debugging.
- **Job to be done**: Navigate massive symbol graphs, understand blast radiuses, review agent-proposed changes, and record architectural feedback without cognitive overload.

### Brand Personality
- **Voice**: Professional, precise, unobtrusive, and highly reliable.
- **Vibe**: Ergonomic, polished, and trustworthy.
- **Anti-references**: Do not make it "edgy", "hacker-themed", or brutally utilitarian. Avoid over-the-top maximalism, glowing "cyberpunk" aesthetics, or confusing abstract metaphors.

### Aesthetic Direction
- **Visual Tone**: Clean, refined, and developer-friendly. It should feel like a premium, native developer tool (similar to the standard set by Zed, Linear, or high-end VS Code themes).
- **Colors**: Soothing, low-eye-strain color palettes (calibrated OKLCH grays). Strict semantic use of colors (Red for errors/poisons, Amber for warnings, Green for success, Blue for info).
- **Typography**: A highly legible, modern sans-serif for the UI, paired perfectly with a top-tier monospace font for code and data nodes.

### Design Principles
1. **Ergonomics over Edge**: Prioritize readability, familiar UI patterns, and accessibility over trying to look "cool" or unconventional.
2. **Progressive Disclosure**: Don't overwhelm the user with data. Use clustering and collapsible views to manage the complexity of large graphs.
3. **Keyboard-First, Mouse-Friendly**: Optimize heavily for keyboard shortcuts and command-palette navigation, but ensure everything is fully discoverable via standard point-and-click UI elements.
4. **Information Clarity**: Ensure text contrast, spacing, and borders clearly delineate hierarchy and relationships within the graph canvas and feedback ledger.
