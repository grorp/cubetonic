use std::f32::consts::PI;
use std::fmt::Debug;

use anyhow::anyhow;
use glam::{I16Vec3, Vec3};
use luanti_core::MapBlockNodes;
use luanti_protocol::LuantiClient;
use luanti_protocol::commands::client_to_server::{
    ClientReadySpec, FirstSrpSpec, GotBlocksSpec, Init2Spec, InitSpec, PlayerPosCommand,
    ToServerCommand,
};
use luanti_protocol::commands::server_to_client::ToClientCommand;
use rand::Rng;
use tokio::sync::mpsc;

// Luanti's "BS" factor
const BS: f32 = 10.0;

pub type FromNetworkEventProxy = winit::event_loop::EventLoopProxy<FromNetworkEvent>;

pub enum FromNetworkEvent {
    Blockdata { pos: I16Vec3, data: MapBlockNodes },
}

// TODO: MapBlockNodes doesn't implement Debug, so #[derive(Debug)] on LuantiClientEvent
// is not possible, but we need Debug implemented for Result ? or .unwrap to work
impl Debug for FromNetworkEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blockdata { pos, data: _ } => f
                .debug_struct("Blockdata")
                .field("pos", pos)
                .field("data", &"...")
                .finish(),
        }
    }
}

pub enum ToNetworkEvent {
    PlayerPos { pos: Vec3, yaw: f32, pitch: f32 },
}

pub struct LuantiClientRunner {
    client: LuantiClient,
    tx: FromNetworkEventProxy,
    rx: mpsc::UnboundedReceiver<ToNetworkEvent>,
}

impl LuantiClientRunner {
    pub fn spawn(
        client: LuantiClient,
        tx: FromNetworkEventProxy,
        rx: mpsc::UnboundedReceiver<ToNetworkEvent>,
    ) {
        let _ = tx;
        let mut runner = LuantiClientRunner { client, tx, rx };
        tokio::spawn(async move { runner.run().await });
    }

    async fn run(&mut self) {
        match self.run_inner().await {
            Ok(()) => (), // unreachable
            Err(err) => {
                println!("Disconnected: {}", err);

                /*
                let mut data = self.data.lock().unwrap();
                data.connected = false;
                */
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
            println!("Waiting for command...");

            tokio::select! {
                command = self.client.recv() => {
                    println!("Received command from server: {:?}", command);
                    let command = command?;
                    self.process_network_command(command)?;
                },

                event = self.rx.recv() => {
                    let event = event.ok_or_else(|| anyhow!("client -> network thread channel is closed"))?;
                    self.process_client_event(event)?;
                },
            }
        }
    }

    fn process_network_command(&mut self, command: ToClientCommand) -> anyhow::Result<()> {
        match command {
            // TODO: check connection/auth state first
            ToClientCommand::Hello(spec) => {
                if spec.auth_mechs.first_srp {
                    // register
                    self.client
                        .send(ToServerCommand::FirstSrp(Box::new(FirstSrpSpec {
                            salt: vec![],
                            verification_key: vec![],
                            is_empty: false, // only used for "disallow empty names"
                        })))?;
                } else {
                    // cannot login as that would require actually implementing srp :)
                    panic!("received unsupported or invalid auth method");
                }
            }

            // TODO: check connection/auth state first
            ToClientCommand::AuthAccept(spec) => {
                self.client
                    .send(ToServerCommand::Init2(Box::new(Init2Spec {
                        lang: Some(String::from("en")),
                    })))?;

                // TODO: wait for item definitions, wait for media announce, request media, wait for media, etc. first

                self.client
                    .send(ToServerCommand::ClientReady(Box::new(ClientReadySpec {
                        major_ver: 0,
                        minor_ver: 1,
                        patch_ver: 0,
                        reserved: 0,
                        full_ver: String::from("Cubetonic 0.1.0"),
                        formspec_ver: Some(8), // corresponds to proto ver 46
                    })))?;
            }

            ToClientCommand::Blockdata(spec) => {
                // TODO: do I or does luanti-protocol do this?
                // TODO: Luanti only sends this after meshgen? batching?
                self.client
                    .send(ToServerCommand::GotBlocks(Box::new(GotBlocksSpec {
                        blocks: vec![spec.pos],
                    })))?;

                self.tx.send_event(FromNetworkEvent::Blockdata {
                    pos: spec.pos,
                    data: MapBlockNodes(spec.block.nodes.nodes),
                })?;
            }
            _ => (),
        }

        Ok(())
    }

    fn process_client_event(&mut self, event: ToNetworkEvent) -> anyhow::Result<()> {
        match event {
            ToNetworkEvent::PlayerPos { pos, yaw, pitch } => {
                self.client
                    .send(ToServerCommand::Playerpos(Box::new(PlayerPosCommand {
                        player_pos: luanti_protocol::types::PlayerPos {
                            position: pos * BS,
                            speed: Vec3::ZERO,
                            pitch: pitch,
                            // stored inverted compared to Luanti, Luanti only
                            // inverts it when applying e.g. in camera.cpp
                            yaw: -yaw,
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
