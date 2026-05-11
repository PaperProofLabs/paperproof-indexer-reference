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
    details_json jsonb,
    updated_at timestamptz not null default now()
);

create index if not exists paperproof_content_refs_blob_idx
    on paperproof_content_refs(blob_id);

create table if not exists paperproof_content_cache (
    blob_id text primary key,
    sha256_hex text,
    byte_len bigint,
    content_type text,
    preview_utf8 text,
    status text not null,
    error text,
    updated_at timestamptz not null default now()
);

create table if not exists domain_artifacts (
    series_id text primary key,
    artifact_code text,
    artifact_type bigint,
    owner text,
    latest_version_id text,
    comments_tree_id text,
    likes_book_id text,
    title text,
    status bigint,
    published_at timestamptz,
    updated_at timestamptz not null default now(),
    raw_json jsonb not null
);

create index if not exists domain_artifacts_type_updated_idx
    on domain_artifacts(artifact_type, updated_at desc);

create index if not exists domain_artifacts_owner_idx
    on domain_artifacts(owner);

create table if not exists domain_versions (
    version_id text primary key,
    series_id text not null,
    artifact_type bigint,
    version bigint,
    content_hash text,
    walrus_blob_id text,
    content_type text,
    created_at timestamptz,
    raw_json jsonb not null
);

create index if not exists domain_versions_series_idx
    on domain_versions(series_id, version);

create table if not exists domain_comments (
    tree_id text not null,
    comment_id bigint not null,
    parent_comment_id bigint,
    series_id text,
    author text,
    content_mode bigint,
    status bigint,
    created_at timestamptz,
    updated_at timestamptz not null default now(),
    raw_json jsonb not null,
    primary key (tree_id, comment_id)
);

create index if not exists domain_comments_series_idx
    on domain_comments(series_id, created_at);

create index if not exists domain_comments_parent_idx
    on domain_comments(tree_id, parent_comment_id);

create table if not exists domain_governance_proposals (
    proposal_id bigint primary key,
    proposal_object_id text,
    proposer text,
    title text,
    action_type bigint,
    proposal_type bigint,
    status bigint,
    yes_votes text,
    no_votes text,
    created_at timestamptz,
    updated_at timestamptz not null default now(),
    raw_json jsonb not null
);

create table if not exists domain_votes (
    proposal_id bigint not null,
    voter text not null,
    side bigint,
    voting_power text,
    claimed boolean not null default false,
    created_at timestamptz,
    updated_at timestamptz not null default now(),
    raw_json jsonb not null,
    primary key (proposal_id, voter)
);

create index if not exists domain_votes_voter_idx
    on domain_votes(voter);

create table if not exists domain_activity (
    event_key text primary key,
    kind text not null,
    actor text,
    series_id text,
    proposal_id bigint,
    tree_id text,
    created_at timestamptz,
    raw_json jsonb not null
);

create index if not exists domain_activity_created_idx
    on domain_activity(created_at desc);

create index if not exists domain_activity_series_idx
    on domain_activity(series_id, created_at desc);

create index if not exists domain_activity_actor_idx
    on domain_activity(actor, created_at desc);

create table if not exists domain_airdrop_scores (
    address text primary key,
    published_artifacts bigint not null default 0,
    versions_added bigint not null default 0,
    comments bigint not null default 0,
    votes bigint not null default 0,
    likes bigint not null default 0,
    score bigint not null default 0,
    reasons_json jsonb not null default '[]'::jsonb,
    updated_at timestamptz not null default now()
);

create table if not exists paperproof_indexer_metrics (
    name text primary key,
    value bigint not null default 0,
    updated_at timestamptz not null default now()
);

create table if not exists paperproof_indexer_metric_samples (
    id bigserial primary key,
    name text not null,
    value bigint not null,
    recorded_at timestamptz not null default now()
);

create index if not exists paperproof_indexer_metric_samples_name_idx
    on paperproof_indexer_metric_samples(name, recorded_at desc);
