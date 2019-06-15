use specs::{Component, HashMapStorage, RunNow, World};
use specs_derive::Component;

use crate::sim;

// Resources
pub struct Step(pub u64);
pub struct Camera(pub na::Similarity2<f32>);
pub struct Cursor {
    pub position: na::Vector2<f32>,
    pub pressed: bool,
}

#[derive(Component)]
#[storage(HashMapStorage)]
pub struct Player;

pub struct State {
    pub world: World,
    input: sim::Input,
}

impl State {
    pub fn new() -> Self {
        let mut world = World::new();
        world.add_resource(Step(0));
        world.add_resource(Camera(na::Similarity2::new(na::zero(), 0.0, 0.1)));
        world.add_resource(Cursor {
            position: na::zero(),
            pressed: false,
        });
        crate::sim::setup(&mut world);
        Self {
            world,
            input: sim::Input::new(),
        }
    }

    pub fn step(&mut self) {
        self.input.run_now(&self.world.res);
        let mut step = self.world.write_resource::<Step>();
        step.0 = step.0.wrapping_add(1);
    }

    /// World units wrt. center of camera
    pub fn move_cursor(&mut self, window_pos: &na::Vector2<f32>) {
        let world = self.world.read_resource::<Camera>().0 * window_pos;
        self.world.write_resource::<Cursor>().position = world;
    }

    pub fn cursor_pressed(&mut self, pressed: bool) {
        self.world.write_resource::<Cursor>().pressed = pressed;
    }
}
