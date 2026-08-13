//! The embedded engine, wearing the service's own vocabulary.
//!
//! The service's rules are written against `server-core`'s repository traits,
//! and they must not know which engine is underneath — that is the whole point
//! of having two. So the traits are implemented here rather than the engine
//! inventing its own surface.
//!
//! The methods are thin by design. Every one of them is "do the durable thing,
//! then map the failure into the vocabulary the caller speaks"; anything more
//! interesting would be a rule, and a rule that lived in one engine would be a
//! rule the other did not have.
//!
//! They are also synchronous underneath. redb is a file, not a network, and
//! wrapping a file read in a future does not make it concurrent — it only
//! makes it look like it might be. The `async` here exists because the trait
//! is shared with an engine that genuinely awaits.

use portalis_nexus_server_core::{
    DeviceId, DeviceRecord, IdentityRepository, RepositoryError, ShareId, ShareMembershipRecord,
    ShareRecord, ShareRepository, ShareSnapshotRecord, UserDirectory, UserId, UserRecord,
};

use crate::embedded::Embedded;

impl UserDirectory for Embedded {
    async fn find_user(&self, user_id: UserId) -> Result<Option<UserRecord>, RepositoryError> {
        self.identity()
            .find_user(user_id)
            .map_err(RepositoryError::from)
    }

    async fn find_user_by_handle(
        &self,
        normalized_username: &str,
        discriminator: &str,
    ) -> Result<Option<UserRecord>, RepositoryError> {
        self.identity()
            .find_user_by_handle(normalized_username, discriminator)
            .map_err(RepositoryError::from)
    }
}

impl IdentityRepository for Embedded {
    async fn insert_registration(
        &self,
        user: UserRecord,
        device: DeviceRecord,
    ) -> Result<(), RepositoryError> {
        // A taken handle and an enrolled device are different answers — one
        // retries with another discriminator, the other does not retry at all
        // — so the engine distinguishes them rather than this mapping guessing.
        self.identity()
            .insert_registration(&user, &device)
            .map_err(RepositoryError::from)
    }

    async fn find_device(
        &self,
        device_id: DeviceId,
    ) -> Result<Option<DeviceRecord>, RepositoryError> {
        self.identity()
            .find_device(device_id)
            .map_err(RepositoryError::from)
    }

    async fn list_devices(&self, user: UserId) -> Result<Vec<DeviceRecord>, RepositoryError> {
        self.identity()
            .list_devices(user)
            .map_err(RepositoryError::from)
    }

    async fn link_device(&self, device: DeviceRecord) -> Result<(), RepositoryError> {
        self.identity()
            .enrol_device(&device)
            .map_err(RepositoryError::from)
    }

    async fn touch_device(
        &self,
        device_id: DeviceId,
        at_unix_ns: u64,
    ) -> Result<(), RepositoryError> {
        self.amend_device(device_id, |device| {
            device.last_authenticated_at_unix_ns = Some(at_unix_ns);
        })
    }

    async fn revoke_device(
        &self,
        device_id: DeviceId,
        at_unix_ns: u64,
    ) -> Result<(), RepositoryError> {
        self.amend_device(device_id, |device| {
            // The first revocation is when authority ended; a second one says
            // the same thing and must not move the time.
            device.revoked_at_unix_ns.get_or_insert(at_unix_ns);
        })
    }
}

impl ShareRepository for Embedded {
    async fn find_share(&self, share_id: ShareId) -> Result<Option<ShareRecord>, RepositoryError> {
        self.collections()
            .find_share(share_id)
            .map_err(RepositoryError::from)
    }

    async fn save_publication(
        &self,
        share: ShareRecord,
        snapshot: ShareSnapshotRecord,
        expected_revision: Option<u64>,
    ) -> Result<(), RepositoryError> {
        self.collections()
            .save_publication(&share, &snapshot, expected_revision)
            .map_err(RepositoryError::from)
    }

    async fn find_snapshot(
        &self,
        share_id: ShareId,
        revision: u64,
    ) -> Result<Option<ShareSnapshotRecord>, RepositoryError> {
        self.collections()
            .find_snapshot(share_id, revision)
            .map_err(RepositoryError::from)
    }

    async fn grant_share_access(
        &self,
        membership: ShareMembershipRecord,
    ) -> Result<(), RepositoryError> {
        self.collections()
            .grant_access(
                membership.share_id,
                membership.user_id,
                membership.granted_at_unix_ns,
            )
            .map_err(RepositoryError::from)
    }

    async fn revoke_share_access(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<(), RepositoryError> {
        self.collections()
            .revoke_access(share_id, user_id)
            .map_err(RepositoryError::from)
    }

    async fn has_share_access(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        self.collections()
            .has_access(share_id, user_id)
            .map_err(RepositoryError::from)
    }

    async fn list_authorized_shares(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ShareRecord>, RepositoryError> {
        self.collections()
            .readable_by(user_id)
            .map_err(RepositoryError::from)
    }

    async fn list_share_members(&self, share_id: ShareId) -> Result<Vec<UserId>, RepositoryError> {
        self.collections()
            .list_members(share_id)
            .map_err(RepositoryError::from)
    }
}

impl Embedded {
    /// Reads a device, changes it, and writes it back.
    ///
    /// A device that is not there is ignored rather than reported: every
    /// caller has already established it exists, and a second answer to a
    /// question already asked is a second thing to keep in step.
    fn amend_device(
        &self,
        device_id: DeviceId,
        change: impl FnOnce(&mut DeviceRecord),
    ) -> Result<(), RepositoryError> {
        let Some(mut device) = self.identity().find_device(device_id)? else {
            return Ok(());
        };
        change(&mut device);
        self.identity()
            .save_device(&device)
            .map_err(RepositoryError::from)
    }
}

impl portalis_nexus_server_core::FriendRepository for Embedded {
    async fn find_friendship(
        &self,
        edge: portalis_nexus_server_core::FriendshipEdge,
    ) -> Result<Option<portalis_nexus_server_core::FriendshipRecord>, RepositoryError> {
        self.friends().find(edge).map_err(RepositoryError::from)
    }

    async fn save_friendship(
        &self,
        record: portalis_nexus_server_core::FriendshipRecord,
        expected_version: u64,
    ) -> Result<(), RepositoryError> {
        self.friends()
            .save(&record, expected_version)
            .map_err(RepositoryError::from)
    }

    async fn list_friendships(
        &self,
        user: UserId,
    ) -> Result<Vec<portalis_nexus_server_core::FriendshipRecord>, RepositoryError> {
        self.friends().list(user).map_err(RepositoryError::from)
    }
}

impl portalis_nexus_server_core::EnvelopeRepository for Embedded {
    async fn put_key_envelope(
        &self,
        envelope: portalis_nexus_server_core::KeyEnvelopeRecord,
    ) -> Result<(), RepositoryError> {
        self.envelopes()
            .put(&envelope)
            .map_err(RepositoryError::from)
    }

    async fn list_key_envelopes(
        &self,
        recipient_device_id: DeviceId,
        after_share_id: Option<ShareId>,
    ) -> Result<portalis_nexus_server_core::KeyEnvelopePage, RepositoryError> {
        self.envelopes()
            .page(recipient_device_id, after_share_id)
            .map_err(RepositoryError::from)
    }
}
