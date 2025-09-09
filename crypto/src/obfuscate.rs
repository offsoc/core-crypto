use crate::prelude::{ClientId, ConversationId, HistorySecret};
use derive_more::{Constructor, From};
use hex;
use log::kv::{ToValue, Value};
use openmls::framing::Sender;
use openmls::group::QueuedProposal;
use openmls::prelude::Proposal;
use sha2::{Digest, Sha256};
use std::{
    fmt::{Debug, Formatter},
    sync::LazyLock,
};

pub(crate) trait Obfuscate {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result;
}

impl Obfuscate for &ConversationId {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(hex::encode(compute_hash(self)).as_str())
    }
}

impl Obfuscate for &ClientId {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(hex::encode(compute_hash(self)).as_str())
    }
}

impl Obfuscate for &Proposal {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Proposal::Add(_) => "Add",
            Proposal::Update(_) => "Update",
            Proposal::Remove(_) => "Remove",
            Proposal::PreSharedKey(_) => "PreSharedKey",
            Proposal::ReInit(_) => "ReInit",
            Proposal::ExternalInit(_) => "ExternalInit",
            Proposal::AppAck(_) => "AppAck",
            Proposal::GroupContextExtensions(_) => "GroupContextExtensions",
        })
    }
}

impl Obfuscate for &QueuedProposal {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        (&self.proposal).obfuscate(f)
    }
}

impl Obfuscate for &Sender {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Sender::Member(leaf_node_index) => write!(f, "Member{leaf_node_index}"),
            Sender::External(external_sender_index) => write!(f, "External{external_sender_index:?}"),
            Sender::NewMemberProposal => write!(f, "NewMemberProposal"),
            Sender::NewMemberCommit => write!(f, "NewMemberCommit"),
        }
    }
}

impl Obfuscate for &HistorySecret {
    fn obfuscate(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HistorySecret")
            .field("client_id", &Obfuscated::from(&self.client_id))
            .field("key_package", &"<secret>")
            .finish()
    }
}

/// We often want logging for some values that we shouldn't know the real value of, for privacy reasons.
///
/// `ConversationId` is a canonical example of such an item.
///
/// This wrapper lets us log a partial hash of the sensitive item, so we have deterministic loggable non-sensitive
/// aliases for all our sensitive values.
#[derive(From, Constructor)]
pub struct Obfuscated<T>(T);

impl<T> Debug for Obfuscated<T>
where
    T: Obfuscate,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        self.0.obfuscate(f)
    }
}

impl<T> ToValue for Obfuscated<T>
where
    T: Obfuscate,
{
    fn to_value(&self) -> Value<'_> {
        Value::from_debug(self)
    }
}

fn compute_hash(bytes: &[u8]) -> [u8; 10] {
    /// Store a per-instantiation salt, so that obfuscated values cannot turn into pseudo-ids.
    ///
    /// This will be regenerated each time the library is instantiated. This will be approximately
    /// once per client instantiation.
    static SALT: LazyLock<[u8; 32]> = LazyLock::new(|| {
        use rand::Rng as _;
        let mut salt = [0; _];
        rand::thread_rng().fill(&mut salt);
        salt
    });

    let mut hasher = Sha256::new();
    hasher.update(*SALT);
    hasher.update(bytes);

    let mut output = [0; 10];
    output.copy_from_slice(&hasher.finalize().as_slice()[0..10]);
    output
}
