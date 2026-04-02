# Spritesheet Format

## Export from Aseprite

1. File → Export Sprite Sheet
2. Layout: Pack or By Rows
3. Output: check "JSON Data", format "Hash"
4. Save as `yourpet.png` (JSON saved as `yourpet.json` automatically)

## JSON Structure

The app supports both Aseprite **hash** format (map of frame names → data) and **array** format (list of frame objects).

Frame tags must include at minimum an `idle` tag. Supported tag names:
- `idle` (required)
- `walk`
- `run`
- `sit`
- `sleep`
- `wake`
- `grabbed`
- `petted`
- `react`
- `fall`
- `thrown`

Tag directions: `forward`, `reverse`, `pingpong`, `pingpong_reverse`

## Community Spritesheets

Free pixel-art pet sprites compatible with Aseprite:
- https://itch.io/game-assets/tag-aseprite (search "pet")
- https://itch.io/game-assets/free/tag-pixel-art (search "cat", "dog", etc.)

## Config

Use `embedded://test_pet` as `sheet_path` to use the bundled test spritesheet.
For custom sheets, use an absolute path: `C:/Users/You/sprites/mycat.json`
(the PNG must be in the same directory with the same filename stem).

## Custom Tag Names

The tag names listed above are the *default* names recognised automatically by the state
machine. If your spritesheet uses different names (e.g. `walk_right` instead of `walk`),
you can map them in the Sprite Editor's **SM Coverage** panel without editing the JSON by
hand.

1. Open the tray menu → **Open…** → **Sprites** tab → select your sprite
2. In the left panel, select the state machine you're using from the **SM:** dropdown
3. The **SM Coverage** section shows each SM state with its status: ✓ resolved,
   ✗ required missing, ○ optional missing; rows with explicit overrides have a
   coloured left border
4. For any state with a ✗ (missing) or the wrong tag, click the dropdown on that row
   and select the tag from your spritesheet
5. Click **Save** to write the mapping to the spritesheet JSON

**Example:** Sheet has `walk_right`, SM expects `walk` → select `walk_right` in the
`walk` row's dropdown.
