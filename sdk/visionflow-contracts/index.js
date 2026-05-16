// @visionflow/contracts — runtime constants.
//
// The type-only re-exports live in index.d.ts. This file provides the small
// set of literals that consumers need at runtime (BroadcastChannel name,
// envelope type discriminator, schema version).
//
// Bump in lockstep with `crates/visionclaw-contracts/src/version.rs`.

"use strict";

Object.defineProperty(exports, "__esModule", { value: true });

exports.AGENT_ACTION_CHANNEL = "visionflow:agent-actions";
exports.AGENT_ACTION_TYPE = "visionflow:agent-action";
exports.SCHEMA_VERSION = 1;
exports.SCHEMA_VERSION_STRING = "v1";
