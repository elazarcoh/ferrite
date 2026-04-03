# Custom Sprites

Ferrite supports Aseprite-exported spritesheets. Export your animation as a spritesheet
with JSON metadata and import it via the tray menu.

## Exporting from Aseprite

1. File → Export Sprite Sheet
2. Layout: Pack or By Rows
3. Output: check **JSON Data**, format **Hash**
4. Save as `yourpet.png` — the JSON is saved automatically as `yourpet.json`

## Importing into Ferrite

Open the tray icon → **Open…** → **Sprites** tab → **Import…**. The spritesheet appears
in the gallery and can be assigned to a pet from the **Config** tab.

## Tag Names and the SM Coverage Panel

The **SM Coverage** panel (left panel of the Sprite Editor) shows which SM states are
matched to animation tags in your spritesheet. A state can match a tag in two ways:

- **Auto-match** (✓ no bar) — the state name and tag name are identical
- **Explicit mapping** (✓ with left bar) — you set an override in the Coverage panel

If a state shows **✗** (red, required) or **○** (yellow, optional), no tag is mapped to
it yet. To fix this:

1. Select the SM from the **SM:** dropdown in the left panel
2. In the **SM Coverage** section, find the unresolved state row
3. Click the dropdown on that row and pick the matching tag from your spritesheet
4. Click **Save**

The mapping is stored in the spritesheet JSON (`meta.smMappings`) alongside your
animation data, so it travels with the file.

For a list of the default recognised tag names, see `assets/README.md`.

## Chromakey (Background Removal)

Some sprite tools export with a solid background color (e.g. green screen) instead
of transparency. Ferrite can remove it automatically at load time.

### How to set it up

1. Open the Sprite Editor (system tray → **Open** → **Sprites** tab → select a sprite)
2. In the left panel, find the **Chromakey** section
3. Check **Enable**
4. Click **Pick**, then click on a background pixel in the spritesheet grid
5. Adjust **Tolerance** if edges look rough (higher = removes more near-matching pixels; 0 = exact match only)
6. Click **Save**

The animation preview updates immediately to show the result with the background removed.

### Notes

- The spritesheet PNG is not modified — the removal is applied at render time
- The setting is stored in `meta.chromakey` in the JSON file alongside the PNG
- To disable, uncheck Enable and Save
