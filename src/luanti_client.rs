use std::f32::consts::PI;
use std::net::SocketAddr;

use anyhow::anyhow;
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
use crate::media::{MediaManager, NodeTextureData};
use crate::meshgen::{MapblockMesh, Meshgen};
use crate::node_def::NodeDefManager;

// Luanti's "BS" factor
const BS: f32 = 10.0;

pub enum ClientToMainEvent {
    PlayerPos(PlayerPos),
    MapblockTextureData(NodeTextureData),
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
    device: wgpu::Device,
    queue: wgpu::Queue,
    main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
    main_rx: mpsc::UnboundedReceiver<MainToClientEvent>,

    state: ClientState,
    client: LuantiClient,
    map: LuantiMap,

    node_def: Option<NodeDefManager>,
    media: Option<MediaManager>,
    meshgen: Option<Meshgen>,
}

impl LuantiClientRunner {
    pub async fn spawn(
        device: wgpu::Device,
        queue: wgpu::Queue,
        main_tx: mpsc::UnboundedSender<ClientToMainEvent>,
        main_rx: mpsc::UnboundedReceiver<MainToClientEvent>,
    ) {
        tokio::spawn(async move {
            let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
            println!("Connecting to Luanti server at {}...", addr);
            let client = LuantiClient::connect(addr).await.unwrap();

            let map = LuantiMap::new();

            let mut runner = LuantiClientRunner {
                device,
                queue,
                main_tx,
                main_rx,

                state: ClientState::Connected,
                client,
                map,

                node_def: None,
                media: None,
                meshgen: None,
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

    fn generate_mapblock_with_neighbors(&self, blockpos: MapBlockPos) {
        assert!(self.state == ClientState::ReadySent);
        let meshgen = self.meshgen.as_ref().unwrap();

        meshgen.submit(&self.map, blockpos, self.map.get_block(&blockpos).unwrap());

        for dir in NEIGHBOR_DIRS {
            if let Some(n_blockpos) = blockpos.checked_add(dir)
                && let Some(n_block) = self.map.get_block(&n_blockpos)
            {
                meshgen.submit(&self.map, n_blockpos, n_block);
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

            // TODO: check state properly
            ToClientCommand::Nodedef(spec) => 'b: {
                if self.state != ClientState::Init2Sent || self.node_def.is_some() {
                    println!("Received Nodedef, invalid for state {:?}", self.state);
                    break 'b;
                }

                println!(
                    "Received {} node definitions",
                    spec.node_def.content_features.len()
                );
                self.node_def = Some(NodeDefManager::from_network(spec.node_def));
            }

            // TODO: check state properly
            ToClientCommand::AnnounceMedia(spec) => 'b: {
                if self.state != ClientState::Init2Sent || self.media.is_some() {
                    println!("Received AnnounceMedia, invalid for state {:?}", self.state);
                    break 'b;
                }

                let mut media = MediaManager::new();
                for item in spec.files {
                    match media.try_add_from_cache(&item.name, &item.sha1_base64) {
                        Ok(found) => {
                            if !found {
                                // TODO: download missing media
                                println!("Missing media file in cache: {}", item.name);
                            }
                        }
                        Err(err) => {
                            println!("Error while adding media file {} from cache: {:?}", item.name, err);
                        }
                    }
                }
                self.media = Some(media);

                // TODO: properly check whether loading is finished before updating state

                self.meshgen = Some(Meshgen::new(
                    self.device.clone(),
                    self.queue.clone(),
                    self.main_tx.clone(),
                    self.node_def.take().unwrap(),
                    self.media.take().unwrap(),
                ));

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
            MainToClientEvent::PlayerPos(pos) => {
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
