# AI agents and the command line

BananaV can be driven by programs: from a terminal or a script through its
**command line**, and by AI agents (Claude and others) through the **Model
Context Protocol** (MCP). Both offer the same tools: open and save
projects, import media, edit tracks and events, add effects and set their
parameters (keyframes too), transitions, titles and other generated media,
markers, envelopes, see a frame as a picture, render, undo and redo, and
read or patch the whole project model. How it is built:
[architecture.md](architecture.md#ai-agents-and-the-command-line).

## Two ways to work

- **A project of their own** (headless): `bananav mcp`, or any command
  without `--live`. BananaV runs without a window; the agent opens or
  makes a project, edits it, and saves it. Nothing of yours changes: your
  settings, keyboard map and plug-ins are read, never written.
- **The project open in BananaV's window** (live): turn on **Options ›
  BananaV Mods › AI agents (MCP)**, then `bananav mcp --live` (or a
  command with `--live`). You see every edit as it happens, each one an
  undo step. Saving, rendering, writing a picture and opening another
  project wait for your **Allow** in a window BananaV shows. Only programs
  on this computer can connect (BananaV listens on 127.0.0.1, with a key
  in your settings folder only you can read); the status bar says when an
  agent is connected.

## Connecting an agent (MCP)

BananaV's MCP server talks over its standard input and output, as MCP's
local servers do: the agent's program starts it.

**Claude Desktop** — in `claude_desktop_config.json` (Settings ›
Developer › Edit Config):

```json
{
  "mcpServers": {
    "bananav": {
      "command": "C:\\Users\\you\\AppData\\Local\\Programs\\BananaV\\BananaV.exe",
      "args": ["mcp", "--live"]
    }
  }
}
```

The **Copy MCP configuration** button under the AI agents box gives this
with your BananaV's own path. Leave out `"--live"` for a project of the
agent's own.

**Claude Code** — in a terminal:

```bash
claude mcp add bananav -- "C:\Users\you\AppData\Local\Programs\BananaV\BananaV.exe" mcp --live
```

Where the program is:

| System | The command |
|---|---|
| Windows | `BananaV.exe` in its folder (the installer's: `%LOCALAPPDATA%\Programs\BananaV`) |
| macOS | `/Applications/BananaV.app/Contents/MacOS/BananaV` |
| Linux | the AppImage itself: `BananaV-….AppImage mcp` |

The server speaks MCP's revisions 2024-11-05 to 2025-11-25 and offers
tools (no resources or prompts). At the handshake it tells the agent how
the tools work (the same as the notes below).

## The command line

On Windows, `bananav-cli.exe` (beside `BananaV.exe`) is the command line
for a terminal; elsewhere the program itself takes the commands
(`bananav info …`, `./BananaV.AppImage info …`). Results come as JSON on
the standard output; progress, notes and errors on the standard error.
Exit codes: 0 done, 1 a tool failed, 2 the command was not understood.

| Command | What it does |
|---|---|
| `info <project>` | Settings, tracks, events, media, markers |
| `render <project> <file>` | Renders it: `--template <id or name>`, `--from <s>`, `--to <s>`, `--overwrite` (without a template, the best for the file's extension at the project's size) |
| `frame <project> <file.png>` | A frame as a picture: `--at <s>`, `--width <pixels>`, `--overwrite` |
| `templates` | The render templates this computer can render |
| `effects` | The effects there are: `--kind video\|audio\|transition\|generator`, `--search <text>` |
| `tools` | Every tool with its arguments (JSON Schema) |
| `call <tool> [<project>]` | Runs one tool: `--args <JSON>` or `--args-file <file>`; `--save [<file>]` keeps the changes |
| `run <script.json> [<project>]` | Runs a list of steps in order: `[{"tool": "…", "arguments": {…}}, …]`; `--save [<file>]` |
| `mcp` | The MCP server on the standard input and output |

`--live` works on the project open in the window instead (with the mod
on). Without `--save` a project's changes are not kept (the command says
so).

```bash
bananav-cli info "C:\Projects\trip.bvproj"
bananav-cli call add_marker "C:\Projects\trip.bvproj" --args "{\"at\": 12.5, \"name\": \"Beach\"}" --save
bananav-cli render "C:\Projects\trip.bvproj" "C:\Renders\trip.mp4"
bananav-cli frame "C:\Projects\trip.bvproj" "C:\Renders\cover.png" --at 3 --width 1280
```

## The tools

| Group | Tools |
|---|---|
| Project | `project_info`, `new_project`, `open_project` (.bvproj, .veg, .otio, .xml, .fcpxml), `save_project` (.bvproj; exports .veg, .otio, .xml, .fcpxml, .edl), `set_project_settings`, `list_media`, `import_media`, `remove_media` |
| Timeline | `list_tracks`, `add_track`, `delete_track`, `set_track`, `list_events`, `add_event`, `move_event`, `trim_event`, `split_event`, `delete_events`, `set_event`, `group_events`, `list_markers`, `add_marker`, `add_region`, `delete_marker`, `list_envelopes`, `set_envelope`, `set_cursor`, `select_events`, `set_loop_region` |
| Effects | `list_available_effects`, `list_effects`, `add_effect`, `remove_effect`, `get_effect`, `set_effect`, `set_transition`, `add_generated_media`, `get_generated_media`, `set_generated_media` |
| Output | `preview_frame` (a picture for the agent), `export_frame`, `list_render_templates`, `render`, `render_status`, `cancel_render` |
| Model and history | `undo`, `redo`, `project_json`, `patch_project`, `list_commands` |
| The window's only | `play`, `stop`, `run_command` (a menu command by its id, on the selection) |

`bananav-cli tools` prints them all with their arguments and what each
does.

## Notes for agents

- **Times** are seconds from the project's start (decimals allowed);
  edits land on frames. **Tracks** are numbered from 0 at the top;
  **events**, **media** and **markers** have ids that the list tools give.
- **Levels**: a video track's or event's level is its opacity (0–1); an
  audio one's is a linear gain, 1 = 0 dB, up to 3.98 (+12 dB). Pan goes
  from −1 (left) to 1 (right).
- **Effects** are named by the ids `list_available_effects` gives: a
  plug-in's identifier (`org.bananav.…` for BananaV's own), `audio:<kind>`
  for BananaV's audio effects, `vst:…` for VST plug-ins. `get_effect`
  lists an effect's parameters with their types, ranges and options;
  `set_effect` takes values by name: numbers, true/false, an option's
  label, colors as `[r, g, b]` from 0 to 1 or `"#rrggbb"`, points as
  `[x, y]`. With `at`, each value becomes a keyframe there (the first one
  also keeps the value the parameter had at its owner's start).
  BananaV's audio effects and VST plug-ins change over time through
  envelopes, not keyframes.
- **Titles**: `add_generated_media` with `org.bananav.TitlesText` and
  `{"text": "…"}` (its other parameters: `get_generated_media`).
- **Anything else** in the project — Pan/Crop, Track Motion, masks,
  busses, every setting — can be read with `project_json` and changed with
  `patch_project` (a JSON Patch). There times are ticks: 254,016,000,000
  a second. A patch that leaves the project inconsistent is refused whole.
- A render returns at once; `render_status` follows it.
- In a `run` script each step is independent: ids made by one step are
  not passed to the next (MCP and `call` give them back to read).

## What stays out

- OpenTimelineIO bundles (`.otioz`, `.otiod`) open only in the window.
- Commands that ask for a file (`run_command` with File › Save As and the
  like) open the window's dialog for the user.
