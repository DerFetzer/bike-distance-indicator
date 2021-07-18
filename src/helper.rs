use asm_delay::{bitrate, AsmDelay};

pub fn get_delay() -> AsmDelay {
    AsmDelay::new(bitrate::U32BitrateExt::mhz(64))
}
