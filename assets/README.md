# Spritesheet Format

## Export from Aseprite

1. File → Export Sprite Sheet
2. Layout: Pack / By Rows
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
