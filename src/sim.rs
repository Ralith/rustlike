use ncollide2d::{
    shape,
    world::{CollisionGroups, CollisionObjectHandle, GeometricQueryType},
};
use specs::shred::PanicHandler;
use specs::{Component, Entities, Entity, Read, VecStorage, Write, WriteStorage};
use specs_derive::Component;

use crate::state::Cursor;

pub type CollisionWorld = ncollide2d::world::CollisionWorld<f32, Option<Entity>>;

#[derive(Component)]
#[storage(VecStorage)]
pub struct Collider(pub CollisionObjectHandle);

pub fn setup(world: &mut specs::World) {
    let collision = CollisionWorld::new(0.01);
    world.add_resource(collision);
    world.register::<Collider>();
}

pub struct Input {
    was_pressed: bool,
}

impl Input {
    pub fn new() -> Self {
        Self { was_pressed: false }
    }
}

impl<'a> specs::System<'a> for Input {
    type SystemData = (
        Entities<'a>,
        Read<'a, Cursor, PanicHandler>,
        Write<'a, CollisionWorld, PanicHandler>,
        WriteStorage<'a, Collider>,
    );

    fn run(&mut self, (entities, cursor, mut collision, mut colliders): Self::SystemData) {
        if cursor.pressed && !self.was_pressed {
            println!("{}", cursor.position);
            let entity = entities.create();
            let shape = shape::ShapeHandle::new(shape::Ball::new(1.0));
            let obj = collision.add(
                na::convert(na::Translation2::from(cursor.position)),
                shape,
                CollisionGroups::new(),
                GeometricQueryType::Contacts(0.0, 0.0),
                Some(entity),
            );
            colliders.insert(entity, Collider(obj.handle())).unwrap();
        }
        self.was_pressed = cursor.pressed;
    }
}
