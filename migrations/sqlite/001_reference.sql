-- Copyright (c) 2026 PaperProof Labs
-- SPDX-License-Identifier: Apache-2.0

-- This reference schema extends the SDK event sink tables with content
-- enrichment tables. Third-party indexers may replace these views entirely.

create table if not exists paperproof_content_refs (
    source_event_key text primary key,
    artifact_id text,
    version_id text,
    blob_id text not null,
    expected_sha256_hex text,
    content_type text,
    status text not null default 'pending',
    details_json text,
    updated_at text not null default current_timestamp
);

create index if not exists paperproof_content_refs_blob_idx
    on paperproof_content_refs(blob_id);

create table if not exists paperproof_content_cache (
    blob_id text primary key,
    sha256_hex text,
    byte_len integer,
    content_type text,
    preview_utf8 text,
    status text not null,
    error text,
    updated_at text not null default current_timestamp
);

create table if not exists domain_artifacts (
    series_id text primary key,
    artifact_code text,
    artifact_type integer,
    owner text,
    latest_version_id text,
    comments_tree_id text,
    likes_book_id text,
    title text,
    status integer,
    published_at text,
    updated_at text not null default current_timestamp,
    raw_json text not null
);

create index if not exists domain_artifacts_type_updated_idx
    on domain_artifacts(artifact_type, updated_at desc);

create index if not exists domain_artifacts_owner_idx
    on domain_artifacts(owner);

create table if not exists domain_versions (
    version_id text primary key,
    series_id text not null,
    artifact_type integer,
    version integer,
    content_hash text,
    walrus_blob_id text,
    content_type text,
    created_at text,
    raw_json text not null
);

create index if not exists domain_versions_series_idx
    on domain_versions(series_id, version);

create table if not exists domain_comments (
    tree_id text not null,
    comment_id integer not null,
    parent_comment_id integer,
    series_id text,
    author text,
    content_mode integer,
    status integer,
    created_at text,
    updated_at text not null default current_timestamp,
    raw_json text not null,
    primary key (tree_id, comment_id)
);

create index if not exists domain_comments_series_idx
    on domain_comments(series_id, created_at);

create index if not exists domain_comments_parent_idx
    on domain_comments(tree_id, parent_comment_id);

create table if not exists domain_governance_proposals (
    proposal_id integer primary key,
    proposal_object_id text,
    proposer text,
    title text,
    action_type integer,
    proposal_type integer,
    status integer,
    yes_votes text,
    no_votes text,
    created_at text,
    updated_at text not null default current_timestamp,
    raw_json text not null
);

create table if not exists domain_votes (
    proposal_id integer not null,
    voter text not null,
    side integer,
    voting_power text,
    claimed integer not null default 0,
    created_at text,
    updated_at text not null default current_timestamp,
    raw_json text not null,
    primary key (proposal_id, voter)
);

create index if not exists domain_votes_voter_idx
    on domain_votes(voter);

create table if not exists domain_activity (
    event_key text primary key,
    kind text not null,
    actor text,
    series_id text,
    proposal_id integer,
    tree_id text,
    created_at text,
    raw_json text not null
);

create index if not exists domain_activity_created_idx
    on domain_activity(created_at desc);

create index if not exists domain_activity_series_idx
    on domain_activity(series_id, created_at desc);

create index if not exists domain_activity_actor_idx
    on domain_activity(actor, created_at desc);

create table if not exists domain_airdrop_scores (
    address text primary key,
    published_artifacts integer not null default 0,
    versions_added integer not null default 0,
    comments integer not null default 0,
    votes integer not null default 0,
    likes integer not null default 0,
    score integer not null default 0,
    reasons_json text not null default '[]',
    updated_at text not null default current_timestamp
);

create table if not exists paperproof_indexer_metrics (
    name text primary key,
    value integer not null default 0,
    updated_at text not null default current_timestamp
);

create table if not exists paperproof_indexer_metric_samples (
    id integer primary key autoincrement,
    name text not null,
    value integer not null,
    recorded_at text not null default current_timestamp
);

create index if not exists paperproof_indexer_metric_samples_name_idx
    on paperproof_indexer_metric_samples(name, recorded_at desc);
