use std::collections::HashMap;
use std::f32::consts::PI;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use base64::Engine;
use base64::engine::DecodePaddingMode;
use glam::Vec3;
use luanti_core::{ContentId, MapBlockNodes, MapBlockPos, MapNode, MapNodePos};
use luanti_protocol::LuantiClient;
use luanti_protocol::commands::client_to_server::{
    ClientReadySpec, FirstSrpSpec, GotBlocksSpec, Init2Spec, InitSpec, PlayerPosCommand,
    ToServerCommand,
};
use luanti_protocol::commands::server_to_client::ToClientCommand;
use rand::Rng;
use tokio::sync::mpsc;

use crate::camera_controller::PlayerPos;
use crate::map::{LuantiMap, NEIGHBOR_DIRS};
use crate::meshgen::{MapblockMesh, MeshgenTask};

// Luanti's "BS" factor
const BS: f32 = 10.0;

pub enum ClientToMainEvent {
    PlayerPos(PlayerPos),
    MapblockMesh(MapblockMesh),
}

pub enum MainToClientEvent {
    PlayerPos(PlayerPos),
}

#[derive(Debug, PartialEq)]
enum ClientState {
    Connected,
    AuthSent,
    Init2Sent,
    ReadySent,
}

pub struct LuantiClientRunner {
    device: Arc<wgpu::Device>,
    main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
    main_rx: mpsc::UnboundedReceiver<MainToClientEvent>,

    state: ClientState,
    client: LuantiClient,
    map: LuantiMap,
    meshgen_pool: rayon::ThreadPool,

    media_paths: HashMap<String, PathBuf>,
}

impl LuantiClientRunner {
    pub async fn spawn(
        device: Arc<wgpu::Device>,
        main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
        main_rx: mpsc::UnboundedReceiver<MainToClientEvent>,
    ) {
        tokio::spawn(async move {
            let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
            println!("Connecting to Luanti server at {}...", addr);
            let client = LuantiClient::connect(addr).await.unwrap();

            let map = LuantiMap::new();

            let meshgen_pool = rayon::ThreadPoolBuilder::new()
                .num_threads(0)
                .thread_name(|index| format!("Meshgen #{}", index))
                .build()
                .unwrap();

            let mut runner = LuantiClientRunner {
                device,
                main_tx,
                main_rx,

                state: ClientState::Connected,
                client,
                map,
                meshgen_pool,

                media_paths: HashMap::new(),
            };
            runner.run().await
        });
    }

    async fn run(&mut self) {
        match self.run_inner().await {
            Ok(()) => unreachable!(),
            Err(err) => {
                println!("Disconnected: {}", err);
            }
        }
    }

    async fn run_inner(&mut self) -> anyhow::Result<()> {
        let mut user_name = String::from("test");
        user_name.push_str(&rand::rng().random_range(0..1000).to_string());

        self.client.send(ToServerCommand::Init(Box::new(InitSpec {
            serialization_ver_max: 29,
            supp_compr_modes: 0, // unused
            min_net_proto_version: 46,
            max_net_proto_version: 46, // appears to be the only version supported by luanti-protocol
            user_name: user_name.clone(),
        })))?;

        loop {
            // println!("Waiting for command...");

            tokio::select! {
                command = self.client.recv() => {
                    // println!("Received command from server: {:?}", command);
                    let command = command?;
                    self.process_network_command(command)?;
                },

                event = self.main_rx.recv() => {
                    let event = event.ok_or_else(|| anyhow!("main_rx is closed"))?;
                    self.process_main_event(event)?;
                },
            }
        }
    }

    fn generate_mapblock(&self, blockpos: MapBlockPos, block: &MapBlockNodes) {
        MeshgenTask::spawn(
            self.device.clone(),
            self.main_tx.clone(),
            &self.map,
            &self.meshgen_pool,
            blockpos,
            block,
        );
    }

    fn generate_mapblock_with_neighbors(&self, blockpos: MapBlockPos) {
        self.generate_mapblock(blockpos, self.map.get_block(&blockpos).unwrap());

        for dir in NEIGHBOR_DIRS {
            if let Some(n_blockpos) = blockpos.checked_add(dir)
                && let Some(n_block) = self.map.get_block(&n_blockpos)
            {
                self.generate_mapblock(n_blockpos, n_block);
            }
        }
    }

    fn process_network_command(&mut self, command: ToClientCommand) -> anyhow::Result<()> {
        match command {
            ToClientCommand::Hello(spec) => 'b: {
                if self.state != ClientState::Connected {
                    println!("Received Hello, invalid for state {:?}", self.state);
                    break 'b;
                }

                if spec.auth_mechs.first_srp {
                    // register
                    self.client
                        .send(ToServerCommand::FirstSrp(Box::new(FirstSrpSpec {
                            salt: vec![],
                            verification_key: vec![],
                            is_empty: false, // only used for "disallow empty passwords"
                        })))?;
                    self.state = ClientState::AuthSent;
                } else {
                    // cannot login as that would require actually implementing srp :)
                    panic!("received unsupported or invalid auth method");
                }
            }

            ToClientCommand::AuthAccept(_spec) => 'b: {
                if self.state != ClientState::AuthSent {
                    println!("Received AuthAccept, invalid for state {:?}", self.state);
                    break 'b;
                }

                self.client
                    .send(ToServerCommand::Init2(Box::new(Init2Spec {
                        lang: Some(String::from("en")),
                    })))?;
                self.state = ClientState::Init2Sent;
            }

            ToClientCommand::AnnounceMedia(spec) => 'b: {
                if self.state != ClientState::Init2Sent {
                    println!("Received AnnounceMedia, invalid for state {:?}", self.state);
                    break 'b;
                }

                let mut cache_path = std::env::home_dir().unwrap();
                cache_path.push(".minetest/cache/media");

                let base64 = base64::engine::GeneralPurpose::new(
                    &base64::alphabet::STANDARD,
                    base64::engine::GeneralPurposeConfig::new()
                        // Luanti encodes without padding (currently)
                        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
                );

                for item in spec.files {
                    // The encoding choices made here are very curious
                    let Ok(sha1_raw) = base64.decode(&item.sha1_base64) else {
                        println!("Invalid base64 {} for {}", item.sha1_base64, item.name);
                        continue;
                    };
                    let sha1_hex = hex::encode(sha1_raw);

                    let path = cache_path.join(sha1_hex);
                    if path.exists() {
                        self.media_paths.insert(item.name, path);
                    } else {
                        // TODO: download missing media
                        println!("Missing media file in cache: {} / {:?}", item.name, path);
                    }
                }

                println!("Found {} media files in cache", self.media_paths.len());

                // TODO: wait for item definitions etc. first
                self.client
                    .send(ToServerCommand::ClientReady(Box::new(ClientReadySpec {
                        major_ver: 0,
                        minor_ver: 1,
                        patch_ver: 0,
                        reserved: 0,
                        full_ver: String::from("Cubetonic 0.1.0"),
                        formspec_ver: Some(8), // corresponds to proto ver 46
                    })))?;
                self.state = ClientState::ReadySent;
            }

            ToClientCommand::MovePlayer(spec) => 'b: {
                if self.state != ClientState::ReadySent {
                    println!("Received MovePlayer, invalid for state {:?}", self.state);
                    break 'b;
                }

                self.main_tx
                    .send(ClientToMainEvent::PlayerPos(PlayerPos {
                        pos: spec.pos / BS,
                        yaw: -spec.yaw,
                        pitch: spec.pitch,
                    }))
                    .unwrap();
            }

            ToClientCommand::Blockdata(spec) => 'b: {
                if self.state != ClientState::ReadySent {
                    println!("Received Blockdata, invalid for state {:?}", self.state);
                    break 'b;
                }

                // TODO: Luanti only sends this after meshgen? batching?
                self.client
                    .send(ToServerCommand::GotBlocks(Box::new(GotBlocksSpec {
                        blocks: vec![spec.pos],
                    })))?;

                let blockpos = MapBlockPos::new(spec.pos).unwrap();
                let block = MapBlockNodes(spec.block.nodes.nodes);
                self.map.insert_block(blockpos, block);
                self.generate_mapblock_with_neighbors(blockpos);
            }

            ToClientCommand::Addnode(spec) => 'b: {
                if self.state != ClientState::ReadySent {
                    println!("Received Addnode, invalid for state {:?}", self.state);
                    break 'b;
                }

                if let Some(blockpos) = self.map.set_node(&MapNodePos(spec.pos), spec.node) {
                    self.generate_mapblock_with_neighbors(blockpos);
                }
            }

            ToClientCommand::Removenode(spec) => 'b: {
                if self.state != ClientState::ReadySent {
                    println!("Received Removenode, invalid for state {:?}", self.state);
                    break 'b;
                }

                const AIR_NODE: MapNode = MapNode {
                    content_id: ContentId::AIR,
                    param1: 0,
                    param2: 0,
                };
                if let Some(blockpos) = self.map.set_node(&MapNodePos(spec.pos), AIR_NODE) {
                    self.generate_mapblock_with_neighbors(blockpos);
                }
            }

            _ => (),
        }

        Ok(())
    }

    fn process_main_event(&mut self, event: MainToClientEvent) -> anyhow::Result<()> {
        match event {
            MainToClientEvent::PlayerPos (pos) => {
                self.client
                    .send(ToServerCommand::Playerpos(Box::new(PlayerPosCommand {
                        player_pos: luanti_protocol::types::PlayerPos {
                            position: pos.pos * BS,
                            speed: Vec3::ZERO,
                            pitch: pos.pitch,
                            yaw: -pos.yaw,
                            keys_pressed: 0,
                            // expected to be max of horizontal and vertical fov
                            // just give a high value so we get much data
                            fov: PI,
                            // just give a high value so we get much data
                            wanted_range: 255,
                            camera_inverted: false,
                            movement_speed: 0.0,
                            movement_direction: 0.0,
                        },
                    })))?;
            }
        }

        Ok(())
    }
}
