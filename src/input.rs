#[derive(Default)]

pub struct InputBuffer {
    input: String,
    position: usize,
}

impl InputBuffer {
    pub fn get_input() -> String {
        self.input
    }
    pub fn get_position() -> usize {
        self.position
    }
    pub fn push_input(new_input: char) {
        self.input.push(new_input);
        self.position += 1;
    }
}
