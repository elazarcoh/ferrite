// Core (pure) modules now in ferrite-core — re-export to preserve paths:
pub use ferrite_core::sprite::animation;
pub use ferrite_core::sprite::sheet;
pub use ferrite_core::sprite::sm_compiler;
pub use ferrite_core::sprite::sm_format;
pub use ferrite_core::sprite::sm_runner;

// Desktop-only:
pub use ferrite_core::sprite::editor_state;
pub mod sm_gallery;
pub mod sprite_gallery;
