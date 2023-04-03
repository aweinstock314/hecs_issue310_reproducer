use hecs::{Archetype, ColumnBatchBuilder, ColumnBatchType};
use serde::{
    de::SeqAccess, de::Visitor, ser::SerializeStruct, ser::SerializeTuple, Deserialize,
    Deserializer, Serialize, Serializer,
};
use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    f32::consts::TAU,
    io::Write,
};
use vek::{Mat4, Vec2};

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Position(pub Vec2<f32>);
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Velocity(pub Vec2<f32>);
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Size(pub Vec2<f32>);
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rotation(#[serde(with = "rotation_serializer")] pub f32);

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Renderable {
    Platform,
    Spikes { count: u32 },
    Sawblade { speed: f32 },
    Protagonist,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct SpawnPoint;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct LevelData;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum MovementState {
    Grounded,
    Airborne,
    OnWall(Vec2<f32>),
    Dashing { ticks: u32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AttackState {
    Idle {
        cooldown: u32,
    },
    Sword {
        dir: Vec2<f32>,
        elapsed: u32,
        collisions: HashSet<hecs::Entity>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimedDI {
    vec: Vec2<f32>,
    remaining: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerPhysicsState {
    pub di_vel: Vec2<f32>,
    pub di_acc: Vec2<f32>,
    pub walljump_di: TimedDI,
    pub sword_di_acc: TimedDI,
    pub sword_di_vel: TimedDI,
    pub jump_budget: f32,
    pub movement_state: MovementState,
    pub attack_state: AttackState,
    pub dash_charges: u32,
    pub dash_cooldown: u32,
    pub facing: Vec2<f32>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct PlayerStats {
    pub cur_health: u32,
    pub max_health: u32,
    pub cur_mana: u32,
    pub max_mana: u32,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum LogicalInput {
    Up,
    Down,
    Left,
    Right,
    Jump,
    Slash,
    Dash,
    Reload,
}

pub enum CollisionShape {
    Circle,
    Square,
}

pub enum CollisionBehavior {
    Solid,
    Hurt,
}

pub struct Collider {
    pub shape: CollisionShape,
    pub behavior: CollisionBehavior,
}

// These are the sizes of the full circle that the arc is a subset of, so the actual hitboxes
// are half of these based on direction
pub const SWORD_SIZE_H: Vec2<f32> = Vec2::new(256.0, 96.0);
pub const SWORD_SIZE_V: Vec2<f32> = Vec2::new(64.0, 192.0);

pub fn sword_hitbox(pos: &Position, size: &Size, dir: Vec2<f32>) -> Mat4<f32> {
    let sword_size = if dir.x.abs() > dir.y.abs() {
        SWORD_SIZE_H / Vec2::new(2.0, 1.0)
    } else {
        SWORD_SIZE_V / Vec2::new(1.0, 2.0)
    };
    Position(pos.0 - (sword_size - (dir * sword_size) - size.0) / 2.0).to_mat4()
        * Size(sword_size).to_mat4()
}

impl AttackState {
    pub fn hitbox(&self, pos: &Position, size: &Size) -> Option<Mat4<f32>> {
        match self {
            AttackState::Idle { .. } => None,
            AttackState::Sword { dir, .. } => Some(sword_hitbox(pos, size, *dir)),
        }
    }
}

impl TimedDI {
    pub fn new(vec: Vec2<f32>, remaining: u32) -> TimedDI {
        TimedDI { vec, remaining }
    }

    pub fn zero() -> TimedDI {
        TimedDI {
            vec: Vec2::zero(),
            remaining: 0,
        }
    }

    pub fn try_value(&self) -> Option<Vec2<f32>> {
        if self.remaining > 0 {
            Some(self.vec)
        } else {
            None
        }
    }

    pub fn value(&self) -> Vec2<f32> {
        self.try_value().unwrap_or_else(Vec2::zero)
    }

    pub fn tick(&mut self) {
        if self.remaining > 0 {
            self.remaining -= 1;
        }
        if self.remaining == 0 {
            self.vec = Vec2::zero();
        }
    }
}

impl Default for PlayerPhysicsState {
    fn default() -> PlayerPhysicsState {
        PlayerPhysicsState::new()
    }
}

impl PlayerPhysicsState {
    pub fn new() -> PlayerPhysicsState {
        PlayerPhysicsState {
            di_vel: Vec2::new(0.0, 0.0),
            di_acc: Vec2::new(0.0, 0.0),
            walljump_di: TimedDI::zero(),
            sword_di_acc: TimedDI::zero(),
            sword_di_vel: TimedDI::zero(),
            jump_budget: 128.0,
            movement_state: MovementState::Grounded,
            attack_state: AttackState::Idle { cooldown: 0 },
            dash_charges: 1,
            dash_cooldown: 0,
            facing: Vec2::new(1.0, 0.0),
        }
    }
    pub fn to_mat4(&self) -> Mat4<f32> {
        Mat4::<f32>::translation_3d((0.5, 0.5, 0.0))
            * Mat4::<f32>::scaling_3d(self.facing.with_y(1.0).with_z(1.0))
            * Mat4::<f32>::translation_3d((-0.5, -0.5, 0.0))
    }
}

impl Default for PlayerStats {
    fn default() -> PlayerStats {
        PlayerStats {
            cur_health: 8,
            max_health: 8,
            cur_mana: 0,
            max_mana: 96,
        }
    }
}

impl Position {
    pub fn to_mat4(self) -> Mat4<f32> {
        Mat4::translation_3d(self.0.with_z(0.0))
    }
}
impl Size {
    pub fn to_mat4(self) -> Mat4<f32> {
        Mat4::scaling_3d(self.0.with_z(1.0))
    }
}
impl Rotation {
    pub fn to_mat4(self) -> Mat4<f32> {
        Mat4::<f32>::translation_3d((0.5, 0.5, 0.0))
            * Mat4::<f32>::rotation_z(self.0)
            * Mat4::<f32>::translation_3d((-0.5, -0.5, 0.0))
    }
}
mod rotation_serializer {
    use super::{Deserialize, TAU};
    pub fn serialize<S: serde::Serializer>(theta: &f32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f32(((360.0 * theta / TAU) * 1000.0).round() / 1000.0)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        Ok(TAU * f32::deserialize(d)? / 360.0)
    }
}

impl Renderable {
    pub fn to_shapedata(self) -> [u32; 4] {
        match self {
            Renderable::Platform => [0, 0x101010ff, 0, 0],
            Renderable::Spikes { count } => [2u32, 0x808080ff, count & 0xff, 0],
            Renderable::Sawblade { .. } => [1u32, 0xffffffff, 0x00001000, 0b101],
            Renderable::Protagonist => [3, 0xffffffff, 0, 0],
        }
    }

    pub fn collider(self) -> Option<Collider> {
        match self {
            Renderable::Platform => Some(Collider {
                shape: CollisionShape::Square,
                behavior: CollisionBehavior::Solid,
            }),
            Renderable::Spikes { .. } => Some(Collider {
                shape: CollisionShape::Square,
                behavior: CollisionBehavior::Hurt,
            }),
            Renderable::Sawblade { .. } => Some(Collider {
                shape: CollisionShape::Circle,
                behavior: CollisionBehavior::Hurt,
            }),
            Renderable::Protagonist => None,
        }
    }
}

macro_rules! enumerate_rollbacked_components {
    ($mac:ident) => {
        $mac! {
            position: Position,
            velocity: Velocity,
            size: Size,
            rotation: Rotation,
            phys: PlayerPhysicsState,
            stats: PlayerStats,
        }
    };
}

pub const ROLLBACK_SIZE: usize = 20;

#[derive(Clone, Debug)]
pub struct RollbackBuffer<T, const N: usize> {
    buffer: [(u64, T); N],
    i: usize,
}

impl<T: Serialize, const N: usize> Serialize for RollbackBuffer<T, N> {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut s = s.serialize_struct("RollbackBuffer", 2)?;
        s.serialize_field("buffer", &self.buffer[..])?;
        s.serialize_field("i", &self.i)?;
        s.end()
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for RollbackBuffer<T, N> {
    fn deserialize<D>(d: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        /*struct RBVisitor<'de>;
        impl<'de> Visitor<'de> for RBVisitor<'de> {
            type Value = RollbackBuffer<T, N>;
            fn expecting(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(fmt, "a RollBackBuffer<T, N>")
            }
        }
        d.deserialize_struct("RollbackBuffer", &["buffer", "i"]*/
        #[derive(Deserialize)]
        #[serde(rename = "RollbackBuffer")]
        struct RollbackBufferVec<T> {
            buffer: Vec<(u64, T)>,
            i: usize,
        }
        let rbv = RollbackBufferVec::deserialize(d)?;
        if rbv.buffer.len() == N {
            let rb = RollbackBuffer {
                buffer: <[(u64, T); N]>::try_from(rbv.buffer)
                    .map_err(|_| serde::de::Error::custom("failed to create array"))?,
                i: rbv.i,
            };
            Ok(rb)
        } else {
            Err(serde::de::Error::invalid_length(
                rbv.buffer.len(),
                &&*format!("{}", N),
            ))
        }
    }
}

impl<T: Default, const N: usize> Default for RollbackBuffer<T, N> {
    fn default() -> RollbackBuffer<T, N> {
        RollbackBuffer {
            buffer: [(); N].map(|()| (0, T::default())),
            i: 0,
        }
    }
}

impl<T: std::fmt::Debug, const N: usize> RollbackBuffer<T, N> {
    pub fn push(&mut self, tick: u64, value: T) {
        self.i += 1;
        self.i %= N;
        self.buffer[self.i] = (tick, value);
    }

    pub fn rollback_to_tick(&mut self, search_tick: u64) -> Option<&T> {
        for j in 0..N {
            let index = (self.i - j - 1 + N) % N;
            let (tick, value) = &self.buffer[index];
            if tick == &search_tick {
                self.i = index;
                return Some(value);
            }
        }
        None
        //panic!("Attempt to roll back more than {} ticks (goal: {}): {:?}.", N, search_tick, self.buffer);
    }
}

pub struct PrettyCompactFormatter {
    pub depth: usize,
}

impl serde_json::ser::Formatter for PrettyCompactFormatter {
    fn begin_object<W: Write + ?Sized>(&mut self, w: &mut W) -> std::io::Result<()> {
        write!(w, "{{")?;
        self.depth += 1;
        Ok(())
    }
    fn end_object<W: Write + ?Sized>(&mut self, w: &mut W) -> std::io::Result<()> {
        if self.depth <= 1 {
            write!(w, "\n")?;
        }
        write!(w, "}}")?;
        self.depth -= 1;
        Ok(())
    }
    fn begin_object_key<W: Write + ?Sized>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        if !first {
            write!(w, ",")?;
        }
        if self.depth <= 1 {
            write!(w, "\n  ")?;
        }
        Ok(())
    }
}

macro_rules! hecs_serialization {
    ($tag:ident, $ser_ctx:ident, $row_de_ctx:ident, $col_de_ctx:ident, [$($ty:ident,)*]) => {
        #[derive(Serialize, Deserialize)]
        pub enum $tag {
            $($ty,)*
        }

        pub struct $ser_ctx;

        impl hecs::serialize::row::SerializeContext for $ser_ctx {
            fn serialize_entity<S>(&mut self, entity: hecs::EntityRef<'_>, mut map: S) -> Result<S::Ok, S::Error>
            where
                S: serde::ser::SerializeMap,
            {
                use hecs::serialize::row::try_serialize;
                $(try_serialize::<$ty, _, _>(&entity, &$tag::$ty, &mut map)?;)*
                map.end()
            }
        }

        impl hecs::serialize::column::SerializeContext for $ser_ctx {
            fn component_count(&self, archetype: &Archetype) -> usize {
                archetype
                    .component_types()
                    .filter(|&t| {
                        false
                        $(|| t == TypeId::of::<$ty>())*
                    })
                    .count()
            }
            fn serialize_component_ids<S: SerializeTuple>(&mut self, archetype: &Archetype, mut out: S) -> Result<S::Ok, S::Error> {
                use hecs::serialize::column::try_serialize_id;
                $(try_serialize_id::<$ty, _, _>(archetype, &$tag::$ty, &mut out)?;)*
                out.end()
            }
            fn serialize_components<S: SerializeTuple>(&mut self, archetype: &Archetype, mut out: S) -> Result<S::Ok, S::Error> {
                use hecs::serialize::column::try_serialize;
                $(try_serialize::<$ty, _>(archetype, &mut out)?;)*
                out.end()
            }
        }

        pub struct $row_de_ctx;

        impl hecs::serialize::row::DeserializeContext for $row_de_ctx {
            #[rustfmt::skip]
            fn deserialize_entity<'de, M: serde::de::MapAccess<'de>>(&mut self, mut map: M, entity: &mut hecs::EntityBuilder) -> Result<(), M::Error> {
                while let Some(key) = map.next_key()? {
                    match key {
                        $($tag::$ty => { entity.add::<$ty>(map.next_value()?); })*
                    }
                }
                Ok(())
            }
        }

        pub struct $col_de_ctx {
            components: Vec<$tag>,
        }

        impl $col_de_ctx {
            pub fn new() -> $col_de_ctx {
                $col_de_ctx { components: Vec::new() }
            }
        }

        impl hecs::serialize::column::DeserializeContext for $col_de_ctx {
            #[rustfmt::skip]
            fn deserialize_component_ids<'de, A>(&mut self, mut seq: A) -> Result<ColumnBatchType, A::Error>
            where
                A: SeqAccess<'de>,
            {
                self.components.clear();
                let mut batch = ColumnBatchType::new();
                while let Some(id) = seq.next_element()? {
                    match id {
                        $($tag::$ty => { batch.add::<$ty>(); })*
                    }
                    self.components.push(id);
                }
                Ok(batch)
            }
            #[rustfmt::skip]
            fn deserialize_components<'de, A>(&mut self, entity_count: u32, mut seq: A, batch: &mut ColumnBatchBuilder) -> Result<(), A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                use hecs::serialize::column::deserialize_column;
                for component in &self.components {
                    match *component {
                        $($tag::$ty => { deserialize_column::<$ty, _>(entity_count, &mut seq, batch)?; })*
                    }
                }
                Ok(())
            }
        }
    }
}

pub type RBPosition = RollbackBuffer<Position, { ROLLBACK_SIZE }>;
pub type RBVelocity = RollbackBuffer<Velocity, { ROLLBACK_SIZE }>;
pub type RBSize = RollbackBuffer<Size, { ROLLBACK_SIZE }>;
pub type RBRotation = RollbackBuffer<Rotation, { ROLLBACK_SIZE }>;
pub type RBPlayerPhysicsState = RollbackBuffer<PlayerPhysicsState, { ROLLBACK_SIZE }>;
pub type RBPlayerStats = RollbackBuffer<PlayerStats, { ROLLBACK_SIZE }>;

hecs_serialization!(
    ComponentTypeTag,
    WorldSerializeContext,
    WorldDeserializeContext,
    WorldDeserializeColumnContext,
    [
        Position,
        Velocity,
        Size,
        Rotation,
        Renderable,
        PlayerPhysicsState,
        PlayerStats,
        SpawnPoint,
        LevelData,
        LogicalInputState,
        BufferedLogicalInputs,
        RBPosition,
        RBVelocity,
        RBSize,
        RBRotation,
        RBPlayerPhysicsState,
        RBPlayerStats,
    ]
);
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LogicalInputState {
    pub keys_held: HashMap<LogicalInput, u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BufferedLogicalInputs {
    pub inputs: HashMap<u64, LogicalInputState>,
}

impl BufferedLogicalInputs {
    pub fn update_rollback_tick(&self, rollback_tick: &mut u64) {
        if let Some(tick) = self.inputs.keys().max() {
            *rollback_tick = (*rollback_tick).min(*tick);
        }
    }
}

fn main() {
    let pre = include_bytes!("pre.json");
    let post = include_bytes!("post.json");
	let current_tick = 0;

	let mut deserializer = serde_json::Deserializer::from_slice(pre);
	let mut world = hecs::serialize::row::deserialize(&mut WorldDeserializeContext, &mut deserializer).unwrap();
	let mut cmd = hecs::CommandBuffer::new(); 
	macro_rules! x {
		($($name:ident: $type:ident,)*) => {
			$(
				for (entity, ($name,)) in world.query_mut::<(&RollbackBuffer<$type, { ROLLBACK_SIZE }>,)>() {
					cmd.insert(entity, ($name.clone(),));
				}
			)*
		}
	}
	enumerate_rollbacked_components!(x);
	let mut rollback_tick = current_tick;
	for (entity, (inputs,)) in world.query_mut::<(&BufferedLogicalInputs,)>() {
		inputs.update_rollback_tick(&mut rollback_tick);
		cmd.insert(entity, (inputs.clone(),));
	}
	rollback_tick = rollback_tick.max(current_tick.saturating_sub(ROLLBACK_SIZE as u64 - 1));
	let mut deserializer = serde_json::Deserializer::from_slice(post);
	world = hecs::serialize::row::deserialize(&mut WorldDeserializeContext, &mut deserializer).unwrap();
	cmd.run_on(&mut world);
}
