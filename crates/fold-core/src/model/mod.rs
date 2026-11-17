mod flags;
mod message;

pub use flags::{FLAG_ANSWERED, FLAG_DELETED, FLAG_DRAFT, FLAG_FLAGGED, FLAG_SEEN, Flags};
pub use message::{
    Account, Attachment, BodyKind, Folder, FolderKind, Label, Message, Outbox, OutboxStatus, Thread,
};
