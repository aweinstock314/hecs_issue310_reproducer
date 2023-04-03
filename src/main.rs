use hecs::{Archetype, ColumnBatchBuilder, ColumnBatchType};
use serde::{
    de::SeqAccess, ser::SerializeStruct, ser::SerializeTuple, Deserialize, Deserializer, Serialize,
    Serializer,
};
use std::{any::TypeId, f32::consts::TAU};
use vek::Vec2;

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct Position(pub Vec2<f32>);
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

mod rotation_serializer {
    use super::{Deserialize, TAU};
    pub fn serialize<S: serde::Serializer>(theta: &f32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f32(((360.0 * theta / TAU) * 1000.0).round() / 1000.0)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        Ok(TAU * f32::deserialize(d)? / 360.0)
    }
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
pub type RBSize = RollbackBuffer<Size, { ROLLBACK_SIZE }>;
pub type RBRotation = RollbackBuffer<Rotation, { ROLLBACK_SIZE }>;

hecs_serialization!(
    ComponentTypeTag,
    WorldSerializeContext,
    WorldDeserializeContext,
    WorldDeserializeColumnContext,
    [
        Position, Size, Rotation, Renderable, SpawnPoint, LevelData, RBPosition, RBSize,
        RBRotation,
    ]
);

fn main() {
    let pre = include_bytes!("pre.json");
    let post = include_bytes!("post.json");

    let mut deserializer = serde_json::Deserializer::from_slice(pre);
    let mut world =
        hecs::serialize::row::deserialize(&mut WorldDeserializeContext, &mut deserializer).unwrap();
    let mut cmd = hecs::CommandBuffer::new();
    for (entity, (rotation,)) in
        world.query_mut::<(&RollbackBuffer<Rotation, { ROLLBACK_SIZE }>,)>()
    {
        cmd.insert(entity, (rotation.clone(),));
    }
    let mut deserializer = serde_json::Deserializer::from_slice(post);
    world =
        hecs::serialize::row::deserialize(&mut WorldDeserializeContext, &mut deserializer).unwrap();
    cmd.run_on(&mut world);
}
