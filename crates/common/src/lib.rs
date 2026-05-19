#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(clippy::all)]

pub mod framing;

pub mod game_common_generated {
    include!("flatbuffers_generated/game_common_generated.rs");

    pub use rand_game::*;
}

pub mod game_input_generated {
    include!("flatbuffers_generated/game_input_generated.rs");
}

pub mod game_output_generated {
    include!("flatbuffers_generated/game_output_generated.rs");
}

pub mod fb {
    pub use crate::game_common_generated::rand_game::*;
    pub use crate::game_input_generated::rand_game::*;
    pub use crate::game_output_generated::rand_game::*;
}
