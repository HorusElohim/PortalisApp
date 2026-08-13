//! Answering a peer that happens to be a service.
//!
//! A service is a peer that also stores. It speaks the same session vocabulary
//! (`client::session::Request`), and a client fetching a device log does not
//! care whether the bytes came from the person who signed it or from something
//! holding a copy. That is only safe because an object is valid on its own
//! terms (§9) — and it is what keeps the peer path from being second class.
//!
//! What this module contains is therefore very little: a match, and the store
//! it reads. Every rule that could be got wrong is on the other side, held by
//! whoever has a key.
//!
//! Note what is *not* here. There is no authorization on a fetch. Anybody may
//! ask for anybody's device log, because a device log is public by
//! construction — it is signed, it names only keys, and hiding it would
//! protect nothing while breaking the one thing a stranger legitimately needs
//! before verifying an invitation. Delivery is bounded rather than
//! authorized, for the same reason: the mailbox holds ciphertext, and a
//! recipient who is sent something they did not want discards it.

use crate::StorageError;
use crate::embedded::Embedded;

/// What a service answers with.
///
/// Bytes, always. The requester knows what it asked for and decodes
/// accordingly, which keeps the service from having a schema for things it
/// cannot read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer(pub Vec<u8>);

/// What the service could not do.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    /// A request only a peer can answer, asked of a service. Fetching a
    /// collection's publication needs the content key to have been sealed to
    /// somebody, which is not something a store can do.
    #[error("a service cannot answer that; ask the peer that published it")]
    NotOurs,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// One service, over one store.
#[derive(Debug)]
pub struct Service {
    store: Embedded,
}

impl Service {
    #[must_use]
    pub const fn new(store: Embedded) -> Self {
        Self { store }
    }

    /// The store beneath, for an operator's tooling and for tests.
    #[must_use]
    pub const fn store(&self) -> &Embedded {
        &self.store
    }

    /// Answers one request from the device identified by `caller`.
    ///
    /// `caller` is the authenticated remote key, which the session established.
    /// It is used to decide *whose* mailbox to drain and nothing else — a
    /// service that let a caller name someone else's mailbox would be a service
    /// that hands anybody anybody's post.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError`] when the request is one only a peer can answer,
    /// or the store fails.
    pub fn answer(
        &self,
        caller: [u8; 32],
        request: &portalis_nexus_client::Request,
    ) -> Result<Answer, ServiceError> {
        use portalis_nexus_client::Request;

        match request {
            // A publication lives with whoever published it. The service
            // carries one in a mailbox when asked to, but it cannot produce
            // one on demand.
            Request::Publication { .. } => Err(ServiceError::NotOurs),

            Request::Collect => {
                let items = self.store.drain(caller)?;
                Ok(Answer(pack(items.into_iter().map(|item| item.body))))
            }
            Request::Deliver { device, body } => {
                self.store.deliver(*device, body)?;
                Ok(Answer(Vec::new()))
            }
            Request::DeviceLog { root_key } => {
                let entries = self.store.fetch_log(root_key)?;
                Ok(Answer(pack(entries.into_iter().map(|(_, entry)| entry))))
            }
        }
    }
}

/// Several blobs in one answer, length-prefixed so one read yields all of them.
fn pack(items: impl Iterator<Item = Vec<u8>>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for item in items {
        bytes.extend_from_slice(&u32::try_from(item.len()).unwrap_or(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(&item);
    }
    bytes
}

/// Reads back what [`pack`] wrote.
///
/// # Errors
///
/// Returns [`StorageError::Malformed`] when the bytes are truncated.
pub fn unpack(bytes: &[u8]) -> Result<Vec<Vec<u8>>, StorageError> {
    let mut items = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (length, tail) = rest.split_at_checked(4).ok_or(StorageError::Malformed)?;
        let length =
            u32::from_be_bytes(<[u8; 4]>::try_from(length).map_err(|_| StorageError::Malformed)?)
                as usize;
        let (item, tail) = tail
            .split_at_checked(length)
            .ok_or(StorageError::Malformed)?;
        items.push(item.to_vec());
        rest = tail;
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use portalis_nexus_client::Request;

    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "portalis-service-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn service(&self) -> Service {
            Service::new(Embedded::open(self.0.join("service.redb")).expect("opens"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ADA: [u8; 32] = [1; 32];
    const MIRA: [u8; 32] = [2; 32];

    #[test]
    fn a_delivery_waits_and_is_collected_by_its_recipient() {
        let scratch = Scratch::new("deliver");
        let service = scratch.service();

        service
            .answer(
                ADA,
                &Request::Deliver {
                    device: MIRA,
                    body: b"a publication".to_vec(),
                },
            )
            .expect("delivers");

        // Ada's own mailbox is empty: she sent, she did not receive.
        assert_eq!(
            service.answer(ADA, &Request::Collect).expect("collects"),
            Answer(Vec::new())
        );

        let collected = service.answer(MIRA, &Request::Collect).expect("collects");
        assert_eq!(
            unpack(&collected.0).expect("unpacks"),
            vec![b"a publication".to_vec()]
        );
    }

    /// The one thing `caller` decides, and the reason it is the authenticated
    /// key rather than a field in the request.
    #[test]
    fn a_caller_can_only_collect_its_own_post() {
        let scratch = Scratch::new("own");
        let service = scratch.service();
        service
            .answer(
                ADA,
                &Request::Deliver {
                    device: MIRA,
                    body: b"for Mira".to_vec(),
                },
            )
            .expect("delivers");

        // There is no way to express "collect Mira's": the request has no
        // field for it, and the caller comes from the session.
        assert_eq!(
            service.answer(ADA, &Request::Collect).expect("collects"),
            Answer(Vec::new())
        );
        assert_eq!(
            unpack(&service.answer(MIRA, &Request::Collect).expect("collects").0)
                .expect("unpacks")
                .len(),
            1
        );
    }

    #[test]
    fn a_device_log_is_served_to_anybody_who_asks() {
        let scratch = Scratch::new("log");
        let service = scratch.service();
        service
            .store()
            .publish_log(&ADA, &[(1, b"root".to_vec()), (2, b"enrol".to_vec())])
            .expect("publishes");

        // A stranger asking is fine: a log is signed, names only keys, and is
        // exactly what somebody needs before verifying an invitation.
        let served = service
            .answer(MIRA, &Request::DeviceLog { root_key: ADA })
            .expect("serves");

        assert_eq!(
            unpack(&served.0).expect("unpacks"),
            vec![b"root".to_vec(), b"enrol".to_vec()]
        );
    }

    #[test]
    fn a_log_nobody_published_is_an_empty_answer_not_a_refusal() {
        let scratch = Scratch::new("nolog");
        let service = scratch.service();

        assert_eq!(
            service
                .answer(MIRA, &Request::DeviceLog { root_key: ADA })
                .expect("serves"),
            Answer(Vec::new())
        );
    }

    /// A service cannot produce a publication: that needs a content key sealed
    /// to somebody, which is not something a store does.
    #[test]
    fn a_publication_is_not_something_a_service_can_be_asked_for() {
        let scratch = Scratch::new("publication");
        let service = scratch.service();

        assert_eq!(
            service.answer(
                ADA,
                &Request::Publication {
                    collection_id: [7; 16]
                }
            ),
            Err(ServiceError::NotOurs)
        );
    }

    #[test]
    fn a_full_mailbox_reaches_the_sender_as_itself() {
        let scratch = Scratch::new("full");
        let service = Service::new(
            Embedded::with_limits(
                scratch.0.join("service.redb"),
                crate::mailbox::Limits {
                    items: 1,
                    bytes: 64,
                },
            )
            .expect("opens"),
        );
        let deliver = Request::Deliver {
            device: MIRA,
            body: b"first".to_vec(),
        };

        service.answer(ADA, &deliver).expect("delivers");

        assert!(matches!(
            service.answer(ADA, &deliver),
            Err(ServiceError::Storage(StorageError::MailboxFull { .. }))
        ));
    }

    #[test]
    fn several_items_pack_and_unpack_in_order() {
        let items = vec![b"one".to_vec(), Vec::new(), b"three".to_vec()];

        let packed = pack(items.clone().into_iter());

        assert_eq!(unpack(&packed).expect("unpacks"), items);
        assert!(unpack(&[]).expect("unpacks").is_empty());
    }

    #[test]
    fn a_truncated_answer_is_reported_rather_than_half_read() {
        let packed = pack(vec![b"one".to_vec()].into_iter());

        for truncated in [&packed[..2], &packed[..5]] {
            assert!(matches!(unpack(truncated), Err(StorageError::Malformed)));
        }
    }
}
