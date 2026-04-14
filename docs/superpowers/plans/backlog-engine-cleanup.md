# Engine Cleanup Backlog

Items identified during the core-computation-centralization refactor that are
out-of-place but are NOT computation (so deferred from that PR per policy).

## B1: DragState encapsulation in wndproc.rs

`HwndData` has 8 loose drag-related fields. Organise them into an inner
`DragState` struct for readability. Pure organisation within Win32 platform code.

File: `src/window/wndproc.rs`

## B2: SpatialContext — deduplicate distance formulas in app.rs

The cursor-to-pet and pet-to-pet distance formulas are written out twice inline
in `App::update()`. A small helper or closure would deduplicate them. Stays in
desktop platform code.

File: `src/app.rs` (~lines 738–751)

## B3: ScaledDimensions — centralise scale rounding

`(value as f32 * cfg.scale).round() as i32` appears in multiple places in
`PetInstance::tick` and `collect_collidables`. A helper type or method would
ensure consistent rounding.

File: `src/app.rs`

## B4: InteractionEvent coordinate frame documentation

`PetDragStart` carries screen-space cursor coordinates; the pet-relative offset
is computed later in `App::handle_event`. A doc comment clarifying the coordinate
frame would prevent confusion when extending drag handling.

File: `src/event.rs`
