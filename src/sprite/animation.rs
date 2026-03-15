use crate::sprite::sheet::{SpriteSheet, TagDirection};

#[derive(Debug, Clone, PartialEq)]
pub enum PlayDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone)]
pub struct AnimationState {
    pub current_tag: String,
    /// Frame index within the tag's range [0, tag.to - tag.from].
    pub frame_index: usize,
    pub elapsed_ms: u32,
    pub ping_pong_dir: PlayDirection,
}

impl AnimationState {
    pub fn new(tag: impl Into<String>) -> Self {
        AnimationState {
            current_tag: tag.into(),
            frame_index: 0,
            elapsed_ms: 0,
            ping_pong_dir: PlayDirection::Forward,
        }
    }

    /// Switch to a new tag and reset playback state.
    pub fn set_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if self.current_tag != tag {
            self.current_tag = tag;
            self.frame_index = 0;
            self.elapsed_ms = 0;
            self.ping_pong_dir = PlayDirection::Forward;
        }
    }

    /// Advance time by `delta_ms`. Returns `true` if the visible frame changed.
    pub fn tick(&mut self, sheet: &SpriteSheet, delta_ms: u32) -> bool {
        let Some(tag) = sheet.tag(&self.current_tag) else {
            return false;
        };
        let tag_len = (tag.to - tag.from + 1).max(1);
        let abs = tag.from + self.frame_index.min(tag_len - 1);
        let frame_dur = sheet.frames.get(abs).map(|f| f.duration_ms).unwrap_or(100).max(1);

        self.elapsed_ms += delta_ms;
        if self.elapsed_ms < frame_dur {
            return false;
        }

        // Advance one or more frames.
        let mut advanced = false;
        while self.elapsed_ms >= frame_dur {
            self.elapsed_ms -= frame_dur;
            advanced = true;
            self.advance_frame(sheet, tag_len);
        }
        advanced
    }

    fn advance_frame(&mut self, sheet: &SpriteSheet, tag_len: usize) {
        let Some(tag) = sheet.tag(&self.current_tag) else { return };

        match tag.direction {
            TagDirection::Forward => {
                self.frame_index = (self.frame_index + 1) % tag_len;
            }
            TagDirection::Reverse => {
                if self.frame_index == 0 {
                    self.frame_index = tag_len - 1;
                } else {
                    self.frame_index -= 1;
                }
            }
            TagDirection::PingPong => {
                if self.ping_pong_dir == PlayDirection::Forward {
                    if self.frame_index + 1 >= tag_len {
                        self.ping_pong_dir = PlayDirection::Backward;
                        if tag_len > 1 {
                            self.frame_index -= 1;
                        }
                    } else {
                        self.frame_index += 1;
                    }
                } else {
                    if self.frame_index == 0 {
                        self.ping_pong_dir = PlayDirection::Forward;
                        self.frame_index += 1;
                    } else {
                        self.frame_index -= 1;
                    }
                }
            }
            TagDirection::PingPongReverse => {
                // Starts going backward, then bounces.
                if self.ping_pong_dir == PlayDirection::Backward {
                    if self.frame_index == 0 {
                        self.ping_pong_dir = PlayDirection::Forward;
                        if tag_len > 1 {
                            self.frame_index += 1;
                        }
                    } else {
                        self.frame_index -= 1;
                    }
                } else {
                    if self.frame_index + 1 >= tag_len {
                        self.ping_pong_dir = PlayDirection::Backward;
                        if tag_len > 1 {
                            self.frame_index -= 1;
                        }
                    } else {
                        self.frame_index += 1;
                    }
                }
            }
        }
    }

    /// Returns the absolute frame index into `sheet.frames`.
    pub fn absolute_frame(&self, sheet: &SpriteSheet) -> usize {
        let Some(tag) = sheet.tag(&self.current_tag) else { return 0 };
        let tag_len = (tag.to - tag.from + 1).max(1);
        tag.from + self.frame_index.min(tag_len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::sheet::load_embedded;

    fn sheet() -> SpriteSheet {
        load_embedded(
            include_bytes!("../../assets/test_pet.json"),
            include_bytes!("../../assets/test_pet.png"),
        )
        .unwrap()
    }

    #[test]
    fn advances_frame_after_duration() {
        let s = sheet();
        let mut anim = AnimationState::new("idle");
        // First tick: not enough time (idle frames are 200 ms each).
        assert!(!anim.tick(&s, 150));
        assert_eq!(anim.frame_index, 0);
        // Second tick: crosses 200 ms.
        assert!(anim.tick(&s, 60));
        assert_eq!(anim.frame_index, 1);
    }

    #[test]
    fn wraps_around() {
        let s = sheet();
        let mut anim = AnimationState::new("idle");
        // idle is pingpong with 200 ms frames: 0 → 1 → 0
        anim.tick(&s, 200); // frame 1
        anim.tick(&s, 200); // pingpong back to 0
        assert_eq!(anim.frame_index, 0);
    }

    #[test]
    fn large_delta_still_advances() {
        let s = sheet();
        let mut anim = AnimationState::new("idle");
        let changed = anim.tick(&s, 500); // >> frame duration
        assert!(changed);
    }

    #[test]
    fn set_tag_resets() {
        let s = sheet();
        let mut anim = AnimationState::new("idle");
        anim.tick(&s, 200); // idle: 200 ms → frame 1
        assert_eq!(anim.frame_index, 1);
        anim.set_tag("idle"); // same tag: no reset
        assert_eq!(anim.frame_index, 1);
        anim.set_tag("walk"); // different tag: reset
        assert_eq!(anim.frame_index, 0);
    }

    #[test]
    fn absolute_frame_correct() {
        let s = sheet();
        let mut anim = AnimationState::new("idle");
        assert_eq!(anim.absolute_frame(&s), 0);
        anim.tick(&s, 200); // idle frames are 200 ms
        assert_eq!(anim.absolute_frame(&s), 1);
    }
}
