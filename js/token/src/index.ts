/**
 * JWT token generation and validation for MoQ authentication.
 *
 * Create and verify JWT tokens used for authorizing publish/subscribe operations in
 * MoQ. Tokens specify which broadcast paths a client can publish to and consume from.
 *
 * See {@link Claims} for the claims structure and {@link Key} for key management.
 * Pattern types from `@moq/pattern` are re-exported for standalone use.
 * Unversioned claims use v0 path prefixes; `v: 1` claims use exact patterns.
 *
 * @module
 */

export {
	compareSpecificity,
	type ErrorCode,
	Pattern,
	PatternError,
	Patterns,
	type Segment,
	type Specificity,
} from "@moq/pattern";
export * from "./algorithm.ts";
export * from "./claims.ts";
export * from "./generate.ts";
export * from "./key.ts";
export * from "./set.ts";
