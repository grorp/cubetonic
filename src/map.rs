use std::collections::HashMap;

use glam::I16Vec3;
use luanti_core::{MapBlockNodes, MapBlockPos, MapNode, MapNodePos};

/// A Luanti map. Consists of "mapblocks", which are 16³ chunks of "nodes".
pub struct LuantiMap {
    blocks: HashMap<MapBlockPos, MapBlockNodes>,
}

impl LuantiMap {
    /// Returns an empty map.
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    /// Inserts a mapblock into the map.
    /// Replaces the mapblock if it already exists.
    pub fn insert_block(&mut self, blockpos: MapBlockPos, data: MapBlockNodes) {
        self.blocks.insert(blockpos, data);
    }

    /// Gets a mapblock from the map.
    /// Returns None if the mapblock doesn't exist.
    pub fn get_block(&self, blockpos: &MapBlockPos) -> Option<&MapBlockNodes> {
        self.blocks.get(blockpos)
    }

    /// Sets a node in the map.
    /// Returns the modified mapblock's position.
    /// Returns None and does nothing if the mapblock that would contain the
    /// node doesn't exist.
    pub fn set_node(&mut self, pos: &MapNodePos, node: MapNode) -> Option<MapBlockPos> {
        let (blockpos, index) = pos.split_index();

        let block = self.blocks.get_mut(&blockpos)?;
        block[index] = node;
        Some(blockpos)
    }
}

/// Offsets for the 6 neighbors of a mapblock or node.
/// Order: +Y, -Y, +X, -Y, +Z, -Z
pub const NEIGHBOR_DIRS: [I16Vec3; 6] = [
    I16Vec3::Y,
    I16Vec3::NEG_Y,
    I16Vec3::X,
    I16Vec3::NEG_X,
    I16Vec3::Z,
    I16Vec3::NEG_Z,
];

/// Stores a clone of a mapblock and its 6 neighbors (if those exist).
/// Used for sending map data to meshgen.
pub struct MeshgenMapData {
    blockpos: MapBlockPos,
    block: MapBlockNodes,
    // Order: see NEIGHBOR_DIRS
    neighbors: [Option<MapBlockNodes>; 6],
}

impl MeshgenMapData {
    /// Creates a new MeshgenMapData, cloning the needed mapblock data.
    pub fn new(map: &LuantiMap, blockpos: MapBlockPos, block: &MapBlockNodes) -> Self {
        let mut result = Self {
            blockpos,
            block: block.clone(),
            neighbors: [const { None }; 6],
        };

        for (index, dir) in NEIGHBOR_DIRS.into_iter().enumerate() {
            if let Some(n_blockpos) = blockpos.checked_add(dir)
                && let Some(n_block) = map.get_block(&n_blockpos)
            {
                result.neighbors[index] = Some(n_block.clone());
            }
        }

        result
    }

    pub fn get_blockpos(&self) -> MapBlockPos {
        self.blockpos
    }

    pub fn get_block(&self) -> &MapBlockNodes {
        &self.block
    }

    /// Returns a node from this mapblock or its neighbors.
    /// Coordinates are relative to the main mapblock.
    /// Returns None if the mapblock that would contain the node doesn't exist
    /// or is outside of the MeshgenMapData's region.
    pub fn get_node(&self, pos: MapNodePos) -> Option<MapNode> {
        let (blockpos, index) = pos.split_index();
        let vec = blockpos.vec();

        if vec == I16Vec3::ZERO {
            return Some(self.block[index]);
        }

        // The inverse of NEIGHBOR_DIRS
        let n_index = match vec {
            pos if pos == I16Vec3::Y => Some(0),
            pos if pos == I16Vec3::NEG_Y => Some(1),
            pos if pos == I16Vec3::X => Some(2),
            pos if pos == I16Vec3::NEG_X => Some(3),
            pos if pos == I16Vec3::Z => Some(4),
            pos if pos == I16Vec3::NEG_Z => Some(5),
            _ => None,
        };
        if let Some(n_index) = n_index {
            return self.neighbors[n_index]
                .as_ref()
                .and_then(|block| Some(block[index]));
        };

        None
    }
}
