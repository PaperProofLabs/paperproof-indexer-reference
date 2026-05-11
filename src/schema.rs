// Copyright (c) 2026 PaperProof Labs
// SPDX-License-Identifier: Apache-2.0

pub const SQLITE_REFERENCE_SCHEMA: &str = include_str!("../migrations/sqlite/001_reference.sql");
pub const POSTGRES_REFERENCE_SCHEMA: &str =
    include_str!("../migrations/postgres/001_reference.sql");
