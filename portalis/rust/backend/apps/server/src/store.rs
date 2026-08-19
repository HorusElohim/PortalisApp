//! The store this server reads and writes.
//!
//! A thin wrapper around the one engine a node runs (ADR-0002), rather than
//! the bare `Embedded` type, so the seam this crate depends on stays named
//! and typed for whatever storage grows next.

use portalis_nexus_server_core::{
    DeviceId, DeviceRecord, EnvelopeRepository, FriendRepository, FriendshipEdge, FriendshipRecord,
    IdentityRepository, KeyEnvelopePage, KeyEnvelopeRecord, RepositoryError, ShareId,
    ShareMembershipRecord, ShareRecord, ShareRepository, ShareSnapshotRecord, UserDirectory,
    UserId, UserRecord,
};

use portalis_nexus_storage::embedded::Embedded;

/// Where durable identity and friend state lives.
///
/// A wrapper rather than a bare `Embedded`: the seam stays named and typed
/// for whatever storage grows next (ADR-0002's Postgres successor), without
/// every caller reaching into the storage crate directly.
#[derive(Debug)]
pub struct NexusStore(Embedded);

impl Default for NexusStore {
    fn default() -> Self {
        Self(Embedded::in_memory().expect("an in-memory store always opens"))
    }
}

impl NexusStore {
    #[must_use]
    pub fn embedded(store: Embedded) -> Self {
        Self(store)
    }
}

impl UserDirectory for NexusStore {
    async fn find_user(&self, user_id: UserId) -> Result<Option<UserRecord>, RepositoryError> {
        self.0.find_user(user_id).await
    }

    async fn find_user_by_handle(
        &self,
        normalized_username: &str,
        discriminator: &str,
    ) -> Result<Option<UserRecord>, RepositoryError> {
        self.0
            .find_user_by_handle(normalized_username, discriminator)
            .await
    }
}

impl IdentityRepository for NexusStore {
    async fn insert_registration(
        &self,
        user: UserRecord,
        device: DeviceRecord,
    ) -> Result<(), RepositoryError> {
        self.0.insert_registration(user, device).await
    }

    async fn find_device(
        &self,
        device_id: DeviceId,
    ) -> Result<Option<DeviceRecord>, RepositoryError> {
        self.0.find_device(device_id).await
    }

    async fn list_devices(&self, user: UserId) -> Result<Vec<DeviceRecord>, RepositoryError> {
        self.0.list_devices(user).await
    }

    async fn link_device(&self, device: DeviceRecord) -> Result<(), RepositoryError> {
        self.0.link_device(device).await
    }

    async fn touch_device(
        &self,
        device_id: DeviceId,
        at_unix_ns: u64,
    ) -> Result<(), RepositoryError> {
        self.0.touch_device(device_id, at_unix_ns).await
    }

    async fn revoke_device(
        &self,
        device_id: DeviceId,
        at_unix_ns: u64,
    ) -> Result<(), RepositoryError> {
        self.0.revoke_device(device_id, at_unix_ns).await
    }
}

impl FriendRepository for NexusStore {
    async fn find_friendship(
        &self,
        edge: FriendshipEdge,
    ) -> Result<Option<FriendshipRecord>, RepositoryError> {
        self.0.find_friendship(edge).await
    }

    async fn save_friendship(
        &self,
        record: FriendshipRecord,
        expected_version: u64,
    ) -> Result<(), RepositoryError> {
        self.0.save_friendship(record, expected_version).await
    }

    async fn list_friendships(
        &self,
        user: UserId,
    ) -> Result<Vec<FriendshipRecord>, RepositoryError> {
        self.0.list_friendships(user).await
    }
}

impl EnvelopeRepository for NexusStore {
    async fn put_key_envelope(&self, envelope: KeyEnvelopeRecord) -> Result<(), RepositoryError> {
        self.0.put_key_envelope(envelope).await
    }

    async fn list_key_envelopes(
        &self,
        recipient_device_id: DeviceId,
        after_share_id: Option<ShareId>,
    ) -> Result<KeyEnvelopePage, RepositoryError> {
        self.0
            .list_key_envelopes(recipient_device_id, after_share_id)
            .await
    }
}

impl ShareRepository for NexusStore {
    async fn find_share(&self, share_id: ShareId) -> Result<Option<ShareRecord>, RepositoryError> {
        self.0.find_share(share_id).await
    }

    async fn save_publication(
        &self,
        share: ShareRecord,
        snapshot: ShareSnapshotRecord,
        expected_revision: Option<u64>,
    ) -> Result<(), RepositoryError> {
        self.0
            .save_publication(share, snapshot, expected_revision)
            .await
    }

    async fn find_snapshot(
        &self,
        share_id: ShareId,
        revision: u64,
    ) -> Result<Option<ShareSnapshotRecord>, RepositoryError> {
        self.0.find_snapshot(share_id, revision).await
    }

    async fn grant_share_access(
        &self,
        membership: ShareMembershipRecord,
    ) -> Result<(), RepositoryError> {
        self.0.grant_share_access(membership).await
    }

    async fn revoke_share_access(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<(), RepositoryError> {
        self.0.revoke_share_access(share_id, user_id).await
    }

    async fn has_share_access(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        self.0.has_share_access(share_id, user_id).await
    }

    async fn list_authorized_shares(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ShareRecord>, RepositoryError> {
        self.0.list_authorized_shares(user_id).await
    }

    async fn list_share_members(&self, share_id: ShareId) -> Result<Vec<UserId>, RepositoryError> {
        self.0.list_share_members(share_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: UserId = [1; 16];
    const GRACE: UserId = [2; 16];

    fn user() -> UserRecord {
        UserRecord {
            user_id: ADA,
            username: "Ada".to_owned(),
            normalized_username: "ada".to_owned(),
            discriminator: "7Q2XZ".to_owned(),
            created_at_unix_ns: 0,
        }
    }

    fn device() -> DeviceRecord {
        DeviceRecord {
            device_id: [1; 32],
            user_id: ADA,
            public_key: [1; 32],
            encryption_public_key: [2; 32],
            created_at_unix_ns: 0,
            last_authenticated_at_unix_ns: None,
            revoked_at_unix_ns: None,
        }
    }

    /// Every method dispatched to the one engine there is, against a real
    /// file. What is checked here is that the wiring reaches it; the
    /// engine's own behaviour is the storage crate's conformance suite.
    #[tokio::test]
    async fn every_operation_reaches_the_embedded_engine() {
        let directory = std::env::temp_dir().join(format!(
            "portalis-store-embedded-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let store = NexusStore::embedded(
            portalis_nexus_storage::embedded::Embedded::open(&directory).expect("opens"),
        );

        store
            .insert_registration(user(), device())
            .await
            .expect("registers");
        assert_eq!(store.find_user(ADA).await.expect("reads"), Some(user()));
        assert_eq!(
            store
                .find_user_by_handle("ada", "7Q2XZ")
                .await
                .expect("reads"),
            Some(user())
        );
        assert_eq!(
            store.find_device(device().device_id).await.expect("reads"),
            Some(device())
        );
        assert_eq!(store.list_devices(ADA).await.expect("reads").len(), 1);
        // A second device joins an account that already exists, which is the
        // one arm registration cannot reach: the first device arrives with the
        // user, so linking is only ever the second one onward.
        store
            .link_device(DeviceRecord {
                device_id: [11; 32],
                public_key: [11; 32],
                ..device()
            })
            .await
            .expect("links");
        assert_eq!(store.list_devices(ADA).await.expect("reads").len(), 2);
        store
            .touch_device(device().device_id, 5)
            .await
            .expect("touches");
        store
            .revoke_device(device().device_id, 6)
            .await
            .expect("revokes");

        collections_friends_and_keys(&store).await;

        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The rest of the dispatch, so every arm is walked rather than the
    /// identity half only.
    async fn collections_friends_and_keys(store: &NexusStore) {
        let share = ShareRecord {
            share_id: [3; 16],
            owner: ADA,
            revision: 1,
            snapshot_id: [4; 32],
            capsule: b"sealed".to_vec(),
            capsule_signature: vec![9; 64],
            created_at_unix_ns: 1,
            updated_at_unix_ns: 1,
        };
        let snapshot = ShareSnapshotRecord {
            share_id: share.share_id,
            revision: 1,
            snapshot_id: share.snapshot_id,
            capsule: share.capsule.clone(),
            capsule_signature: share.capsule_signature.clone(),
            created_at_unix_ns: 1,
        };
        store
            .save_publication(share.clone(), snapshot.clone(), None)
            .await
            .expect("publishes");
        assert_eq!(
            store.find_share(share.share_id).await.expect("reads"),
            Some(share.clone())
        );
        assert_eq!(
            store.find_snapshot(share.share_id, 1).await.expect("reads"),
            Some(snapshot)
        );
        store
            .grant_share_access(ShareMembershipRecord {
                share_id: share.share_id,
                user_id: GRACE,
                granted_at_unix_ns: 1,
            })
            .await
            .expect("grants");
        assert!(
            store
                .has_share_access(share.share_id, GRACE)
                .await
                .expect("reads")
        );
        assert_eq!(
            store
                .list_share_members(share.share_id)
                .await
                .expect("reads"),
            vec![ADA, GRACE],
            "the owner is granted access automatically on first publication"
        );
        assert_eq!(
            store.list_authorized_shares(GRACE).await.expect("reads"),
            vec![share.clone()]
        );
        store
            .revoke_share_access(share.share_id, GRACE)
            .await
            .expect("revokes");

        friends_and_keys(store, share.share_id).await;
    }

    /// Friendships and sealed keys, split out only because the whole walk is
    /// longer than one function should be.
    async fn friends_and_keys(store: &NexusStore, share_id: ShareId) {
        let edge = FriendshipEdge::between(ADA, GRACE).expect("two people");
        let friendship = FriendshipRecord {
            edge,
            requested_by: ADA,
            state: portalis_nexus_protocol::v1::FriendshipState::Pending,
            version: 1,
            created_at_unix_ns: 1,
            updated_at_unix_ns: 1,
        };
        store
            .save_friendship(friendship.clone(), 0)
            .await
            .expect("saves");
        assert_eq!(
            store.find_friendship(edge).await.expect("reads"),
            Some(friendship.clone())
        );
        assert_eq!(
            store.list_friendships(ADA).await.expect("reads"),
            vec![friendship]
        );

        store
            .put_key_envelope(KeyEnvelopeRecord {
                share_id,
                recipient_device_id: device().device_id,
                ephemeral_public_key: [5; 32],
                ciphertext: b"sealed key".to_vec(),
                created_at_unix_ns: 1,
            })
            .await
            .expect("stores");
        assert_eq!(
            store
                .list_key_envelopes(device().device_id, None)
                .await
                .expect("reads")
                .envelopes
                .len(),
            1
        );
    }
}
