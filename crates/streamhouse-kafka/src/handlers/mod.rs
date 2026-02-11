//! Kafka API handlers
//!
//! Each handler processes a specific Kafka API request and returns the response.

mod add_offsets_to_txn;
mod add_partitions_to_txn;
mod api_versions;
mod consumer_groups;
mod end_txn;
mod fetch;
mod init_producer_id;
mod metadata;
mod offsets;
mod produce;
mod topics;
mod txn_offset_commit;

pub use add_offsets_to_txn::handle_add_offsets_to_txn;
pub use add_partitions_to_txn::handle_add_partitions_to_txn;
pub use api_versions::handle_api_versions;
pub use consumer_groups::{
    handle_describe_groups, handle_find_coordinator, handle_heartbeat, handle_join_group,
    handle_leave_group, handle_list_groups, handle_sync_group,
};
pub use end_txn::handle_end_txn;
pub use fetch::handle_fetch;
pub use init_producer_id::handle_init_producer_id;
pub use metadata::handle_metadata;
pub use offsets::{handle_list_offsets, handle_offset_commit, handle_offset_fetch};
pub use produce::handle_produce;
pub use topics::{handle_create_topics, handle_delete_topics};
pub use txn_offset_commit::handle_txn_offset_commit;
