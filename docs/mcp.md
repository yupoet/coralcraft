# CoralCraft MCP server

`coralcraft-cli mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio
(newline-delimited JSON-RPC 2.0). The desktop binary serves the same protocol with `coralcraft --mcp`.
Agents use it to create, edit, inspect and export CoralCraft documents in an in-process, headless
engine session. Only protocol messages are written to stdout.

## Build and register

```sh
cargo build --release -p coralcraft-cli
claude mcp add coralcraft -- "$PWD/target/release/coralcraft-cli" mcp
```

Other clients use the same command in their JSON config:

```json
{"mcpServers": {"coralcraft": {"command": "/abs/path/target/release/coralcraft-cli", "args": ["mcp"]}}}
```

## Protocol

- Requests: `initialize`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/templates/list`, `resources/read`, `prompts/list`, `prompts/get`, `completion/complete`, and `logging/setLevel`.
- Notifications (messages without an `id`, including `notifications/initialized`, `notifications/cancelled`, and `notifications/progress`) follow the server's notification rules; client notifications do not receive responses.
- Supported protocol versions are `2026-07-28`, `2025-06-18` (the fallback), `2025-03-26`, and `2024-11-05`.
- Every tool publishes an `inputSchema`. Tool failures and engine command rejections return tool results with `isError: true`.
- The `2026-07-28` revision adds completion metadata to list and read results. The server enforces line, batch, queue, and output limits covered by `crates/mcp/tests/protocol_props.rs`.

## Verification

- `cargo test -p vectorcraft-mcp` covers protocol behaviour, tool schemas, headless commands, and file I/O.
- `cargo test -p vectorcraft-mcp --test docs_sync` fails when this page's generated tool list or documented protocol surface is stale.

## Tool reference

<!-- BEGIN GENERATED TOOL LIST (cargo test -p vectorcraft-mcp --test docs_sync) -->

33 tools, sorted by name.

| Tool | Description |
|---|---|
| `add_text` | Create point type at (x, y) (the first baseline), area type when width/height are given (`fit` autoHeight grows the frame to the text, shrinkText scales the text down to fit), or type in/on an existing path (`path` id + `mode` area\|onPath; `pathEffect` rainbow\|skew\|3dRibbon\|stairStep\|gravity). Returns the text object id, with `overflow: true` when the text doesn't fit its frame or path (the lines past it aren't drawn: enlarge the frame, shrink the text or give `fit`). |
| `answer_dialog` | Set fields of the desktop app's open dialog in order, then confirm (OK) or cancel it, as the control protocol's ui.dialog.set, ui.dialog.confirm and ui.dialog.cancel do. The app stores all the fields or none. When the app refuses the ui.dialog.set, for example because no dialog is open, the call is an error and the dialog is not confirmed or canceled. inspect_ui shows the open dialog and its fields (ui.dialog). After invoke_menu app.quit or file.close replies {"pending":"saveChanges"}: {action:"confirm"} saves, {set:[{field:"discard",value:true}],action:"confirm"} closes without saving, {action:"cancel"} cancels. Returns {dialog, result}: the dialog after the fields were set, and the reply to confirm or cancel. Desktop app only. |
| `apply_effect` | Append a live effect (Effect menu) to the selection or `ids`. Omit `effect` to get the catalogue (ids, parameters, defaults). Examples: stylize.dropShadow {x, y, blur, opacity}, distort.roughen, warp.arc {bend}, path.offsetPath {offset}, pathfinder.add (on groups), blur.gaussian {radius}. |
| `command_batch` | Run steps in order; each edit is its own undo step, or with atomic:true the whole batch is one (named `label`, else after its first edit; `atomic` says which documents). A step's id is a command id, or draw_path / draw_shape with that tool's arguments as params (its result is the tool's: the new object's `id`). A param string that is exactly "$N.path" (N: 0-based step index) or "$last.path" takes that value from an earlier step's result, e.g. {"id":"$1.id"}; "$$" starts a literal "$". Returns completed, failed, results and created (the ids draw_path / draw_shape steps made, in order). Stops on the first error by default. At most 256 steps; kept step results total at most 8 MiB, past which the batch stops with the reason. |
| `command_list` | Discover command ids, labels, parameter descriptions and enabled state. Returns an array. The whole catalogue is large: narrow it with filter (or query), menu, and brief. list_commands lists the same commands; it takes enabledOnly instead of enabled_only and returns {count, commands}. |
| `command_run` | Run an engine command by id with its JSON params. Command parameters follow the registry's descriptions. run_command runs it the same way; it takes the id as command instead of id. |
| `create_graph` | Create a graph (graph tools) in the plot rectangle. Data as `csv` (first row: empty cell then series names; then one row per category: label, values…) or `series`/`categories`/`rows`. An empty CSV cell or a null in `rows` is a blank value (no column; lines break around it); a number in straight quotes is a label. Edit later with run_command graph.setData / graph.setType. |
| `doc_inspect` | Summary of the active document, including artboards, layer tree, selection and history. A node below full opacity reports `opacity` as a 0–1 fraction and `opacityPct` in the 0–100 % scale the drawing tools and object.setProps take: pass `opacityPct` back, never `opacity`. On a large document scope it: depth 0 for the layer skeleton, ids for just those nodes, kind for every node of one kind, limit/offset to page. inspect_document returns the same summary. |
| `draw_path` | Create a path from points ([[x,y],…] or [{x,y,in?:[x,y],out?:[x,y]},…] with Bézier handles) or SVG path data `d`, then apply optional paint, stroke dash/cap/join, opacity, name and live effects. Omitted paint fields inherit the current new-art appearance (initially white fill and 1 pt black stroke). For a filled path without an outline, pass an explicit fill and stroke:"none". Returns the new object id. |
| `draw_shape` | Create a live shape and apply optional paint. rectangle/ellipse: x, y, width, height (rectangle: radius = corner radius). polygon: cx, cy, radius, sides. star: cx, cy, radius1, radius2, points. line: x1, y1, x2, y2. Omitted paint fields inherit the current new-art appearance (initially white fill and 1 pt black stroke); lines start with no fill and a visible stroke. For a filled shape without an outline, pass an explicit fill and stroke:"none". Stroke dash/cap/join, opacity, name and live effects apply to the new shape as in draw_path. Returns the new object id. |
| `export` | Export the active document with the engine's document.export (the same bytes in the app and headless), in any format of the `format` enum: vectors (svg/svgz with the artboard as viewBox, eps, dxf, emf, wmf, cdr and cmx (CorelDRAW containers)), pdf and ai (one page per artboard: all, or `artboard` / `range`; ai is the PDF-compatible .ai file that Save writes, which carries the document when every artboard is written in order), rasters (one rendered artboard: png, jpg, webp, gif, png8 as an indexed .png, tiff, bmp, tga, psd with its layers), txt (the stories, back to front), vectorcraft or template (a native copy or template; the document keeps its path). run_command document.formats lists each format's options. For SVG/SVGZ, top-level `outlineText: true` writes text as paths; DXF takes it too (its type is outlined anyway unless `preserve: editability`). PDF instead uses `options.advanced.outlineText: true`, default false (live text). `selection: true` exports only the selected objects, cropped to their bounds. Template layers are left out; live effects are kept (geometry baked, SVG filters for shadows/glows/blur). Without `path` the bytes come back as dataBase64. |
| `inspect_document` | Summary of the active document: artboards, layer tree (ids, names, kinds, bounds, fill/stroke; `opacity` as a 0–1 fraction and `opacityPct` in the 0–100 % scale the drawing tools take), selection, undo history, current tool and default paint. On a large document pass depth: 0 for the layer skeleton (truncated levels report childCount), then locate objects with run_command document.find and read them with document.node. doc_inspect returns the same summary. |
| `inspect_ui` | UI state of the running desktop app: tool and options, panels, dialogs, view (zoom/center), window, documents, perf. Desktop app only. The same call as ui_inspect. |
| `invoke_menu` | Invoke a menu item by its command id (same ids as list_commands, including UI-only commands like view.zoomIn or window.* in the desktop app). |
| `list_commands` | List VectorCraft commands (id, label, menu path, shortcut, parameter description, enabled state). Every editing action is a command; run any of them with run_command. Coordinates are points in document space, y down, origin at the first artboard's top-left. command_list lists the same commands as a bare array and takes enabled_only. |
| `open_file` | Open a file as a new, active document: .vectorcraft, .drawcraft, .svg, .svgz, .pdf, .ai, .ait, .png, .pns, .jpg, .jpeg, .gif, .webp, .tif, .tiff, .bmp, .dib, .rle, .tga, .vda, .icb, .vst, .pcx, .pxr, .jpf, .jpx, .jp2, .j2k, .j2c, .jpc, .psd, .pdd, .psb, .vctemplate, .dxf, .emf, .wmf, .eps, .epsf, .ps, .af, .afdesign, .afpub, .cdr, .cmx, .coralcraft (see run_command document.formats). Templates (.ait, native templates) open as a new untitled document. The reply's `warnings` say what didn't come in as it was (an EPS shown as its preview image says why its PostScript couldn't be read). |
| `open_panel` | Open a panel in the desktop app's dock by id (layers, swatches, stroke, align, pathfinder, transform, …; case-insensitive, display labels like "Layers" work too). Desktop app only. |
| `pathfinder` | Combine the selected (or `ids`) objects destructively, back to front: unite, minusFront, intersect, exclude, divide, trim, merge, crop, outline, minusBack. For a non-destructive version apply the pathfinder.* effect to a group. |
| `pointer_gesture` | Drive the active tool with mouse events in document coordinates, exactly like the mouse would (e.g. a rectangle drag: down at (100,100), drag to (200,150), up at (200,150)). Optionally selects `tool` first. |
| `press_key` | Send a key press with modifiers, e.g. {key:"Z", mods:{cmd:true}} (undo), {key:"Escape"}, {key:"Enter"}. In the desktop app it goes through the real keyboard path; headless, it runs the command or tool bound to that shortcut. |
| `redo` | Redo the last undone change (Edit → Redo): the change the last undo took back. |
| `render_preview` | Render an artboard (default the first) as inline PNG without changing the document. max_side defaults to 1024, limited to 1..4096. |
| `run_command` | Execute any VectorCraft command by id with JSON params (see list_commands for ids and params). Examples: {"command":"object.group"}, {"command":"object.align","params":{"horizontal":"left"}}, {"command":"file.new","params":{"width":800,"height":600}}. Returns the command's result (e.g. {id} for creation commands). command_run runs it the same way and takes the id as id. |
| `save_file` | Save the active document with the engine's document.save: by default to its own file in its own format (native .vectorcraft unless it was opened from or saved as SVG, PDF or a restorable .ai). With `path`, the extension picks the format: .vectorcraft, .vctemplate, .pdf, .svg, .svgz, .ai, .eps, .cdr, .cmx. Other formats are exports (see export). A native file is gzipped when `compress` is true (default: the useCompression preference, Preferences > File Handling > Use Compression); `version` writes it for an older VectorCraft, `preview` embeds a PNG of the first artboard, and `precision` rounds path anchors and handles to that many decimal places for that save only. `options` carries the format's save options (see run_command document.formats); giving `compress`, `version`, `preview` or `precision` both at the top level and in `options` with different values is an error. The reply's `format` says what was written and `warnings` what that format loses and each option given that the format doesn't read (such as `compress` for .svg). In the running app the call returns once the file is written, or with the error, even with Background Save on. When the save asks first (before replacing a file that opened with losses, or with a file dialog for an untitled document without `path`), the reply is `pending` or "cancelled" as before. |
| `screenshot` | Render the active artboard to PNG and return it as an image (optionally also saving it to `path`). With window:true (desktop app only) captures the whole app window instead, as ui_screenshot does; max_side then scales the returned image down (a file written to `path` keeps the full capture). |
| `select_tool` | Switch the active tool (as clicking it in the Tools panel). Ids: selection, directSelection, groupSelection, pen, rectangle, roundedRectangle, ellipse, polygon, star, lineSegment, … |
| `set_paint` | Set fill, stroke and/or stroke weight on the selection (or `ids`) and as the default for new art. Returns {applied, objects}: `objects` counts the objects painted, as paint.setFill, paint.setStroke and stroke.set report them (the largest of their counts); when it is 0, a `note` says that only the default for new art changed. |
| `text_wrap` | Make the selected (or `ids`) objects wrap objects: area type below them in the same layer flows around them (offset in pt, invert = flow inside). `release: true` removes the wrap. |
| `transform` | Move, rotate, scale, reflect or shear the selection (or `ids`); several may be combined and run in that order. Angles in degrees (counter-clockwise), scale in %. `origin` [x,y] defaults to the selection centre; `copy` transforms a copy. |
| `type_text` | Type text into the focused widget (text tool, dialog field). Desktop app only. |
| `ui_inspect` | Inspect the connected desktop interface; returns a tool error in headless mode. The same call as inspect_ui. |
| `ui_screenshot` | Capture the connected desktop window as inline PNG; requires a running desktop app. The same call as screenshot with window:true. max_side scales the returned image down to at most that many pixels on its longer side (1..4096; default: the full capture). |
| `undo` | Undo the last change (Edit → Undo). redo reapplies it. |

<!-- END GENERATED TOOL LIST -->
