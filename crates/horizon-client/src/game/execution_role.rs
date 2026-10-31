//! A Game's execution role is fixed at construction. Connecting or disconnecting
//! changes `remote`, not this role. A client may simulate a world in-frame.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ExecutionRole {
    Client,
    IntegratedServer,
    DedicatedServer,
}

impl ExecutionRole {
    /// Only a client owns presentation work (audio, loading UI, chunk meshes).
    pub fn is_client(self) -> bool {
        self == Self::Client
    }

    /// Only a dedicated server skips writing the world owner snapshot.
    /// Integrated servers also set Options.dedicated for simulation compatibility.
    pub fn saves_world_player(self) -> bool {
        self != Self::DedicatedServer
    }

    /// The optional in-frame simulation remains a client execution mode.
    pub fn starts_integrated_server(self, server_thread: bool) -> bool {
        self.is_client() && server_thread
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionRole::*;

    #[test]
    fn world_loading_keeps_in_frame_clients_and_server_roles_separate() {
        for (role, client, threaded) in [
            (Client, true, true),
            (IntegratedServer, false, false),
            (DedicatedServer, false, false),
        ] {
            assert_eq!(role.is_client(), client);
            assert_eq!(role.starts_integrated_server(true), threaded);
            assert!(!role.starts_integrated_server(false));
        }
    }
    #[test]
    fn integrated_world_reopen_keeps_inventory_position_and_experience() {
        use horizon_client::save::Level;
        let root = std::env::temp_dir().join(format!("horizon-owner-save-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut level = Level {
            position: Some([12.5, 80.0, -7.5]),
            xp_level: 9,
            inventory: vec![(0, "minecraft:diamond".into(), 7, 0)],
            ..Default::default()
        };
        for role in [Client, IntegratedServer, DedicatedServer] {
            let dir = root.join(format!("{role:?}"));
            if role.saves_world_player() {
                level.write(&dir).unwrap();
            } else {
                level.write_without_player(&dir).unwrap();
            }
            let read = Level::read(&dir).unwrap();
            if role.saves_world_player() {
                assert_eq!(read.position, level.position);
                assert_eq!(read.inventory, level.inventory);
                assert_eq!(read.xp_level, 9);
            } else {
                assert!(read.position.is_none());
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
