import { expect, test } from "bun:test";
import { authorize, type Claims, ClaimsSchema, type ClaimsV0, scopeAllows } from "./claims.ts";

// These cases mirror the Rust moq-token crate's claims::tests one-for-one, so both
// sides stay pinned to the same authorization semantics.

function claims(root: string, get: string[], put: string[]): Claims {
	return { root, get, put };
}

test("claims granting nothing are rejected, matching Rust's useless-token rule", () => {
	// An empty list grants nothing, so the Rust crate refuses to verify such a token
	// ("no publish or subscribe allowed; token is useless"). Presence alone is not enough.
	expect(() => ClaimsSchema.parse({ root: "demo", put: [] })).toThrow();
	expect(() => ClaimsSchema.parse({ root: "demo", put: [], get: [] })).toThrow();
	expect(() => ClaimsSchema.parse({ root: "demo" })).toThrow();

	// A grant of "" is the root itself, which is a real grant.
	expect((ClaimsSchema.parse({ root: "demo", put: [""] }) as ClaimsV0).put).toEqual([""]);
	expect((ClaimsSchema.parse({ root: "demo", get: "" }) as ClaimsV0).get).toBe("");
});

test("authorize - path equals root", () => {
	const permissions = authorize(claims("room/123", [""], ["alice"]), "room/123");
	expect(permissions).toEqual({ subscribe: [""], publish: ["alice"] });
});

test("authorize - path extends root", () => {
	// Connecting below the root consumes the matching part of each grant.
	const permissions = authorize(claims("room/123", ["bob"], ["alice"]), "room/123/alice");
	expect(permissions).toEqual({ subscribe: [], publish: [""] });
});

test("authorize - path is parent of root", () => {
	// Connecting above the root prepends it, keeping the grants anchored.
	const permissions = authorize(claims("demo", [""], ["alice"]), "/");
	expect(permissions).toEqual({ subscribe: ["demo"], publish: ["demo/alice"] });
});

test("authorize - empty root", () => {
	// A root-scoped token grants everything it lists, wherever it connects.
	const permissions = authorize(claims("", ["demo"], []), "demo/room");
	expect(permissions).toEqual({ subscribe: [""], publish: [] });
});

test("authorize - slashes are implicit", () => {
	const permissions = authorize(claims("/room/123/", ["/bob/"], []), "//room/123//");
	expect(permissions.subscribe).toEqual(["bob"]);
});

test("authorize - accepts a single path as well as a list", () => {
	const permissions = authorize({ root: "demo", get: "bob", put: "alice" }, "demo");
	expect(permissions).toEqual({ subscribe: ["bob"], publish: ["alice"] });
});

test("authorize - respects segment boundaries", () => {
	// "foo" must not cover "foobar".
	expect(() => authorize(claims("foo", [""], [""]), "foobar")).toThrow(/does not overlap/);
});

test("authorize - unrelated path", () => {
	expect(() => authorize(claims("demo", [""], [""]), "other")).toThrow(/does not overlap/);
});

test("authorize - no access at path", () => {
	// The path overlaps the root, but every grant sits outside it.
	expect(() => authorize(claims("", ["demo"], []), "other")).toThrow(/grants no access/);
});

test("authorize - returns reduced v0 sets", () => {
	const permissions = authorize({ root: "demo", get: ["a", "a/b", "b"] }, "demo");
	expect(permissions.subscribe).toEqual(["a", "b"]);
});

test("v1 wire shape matches the quest", () => {
	const parsed = ClaimsSchema.parse({ v: 1, root: "pid", publish: ["*/chat"], subscribe: ["**/*.hang"] });
	if (!("v" in parsed) || parsed.v !== 1) throw new Error("expected v1");
	expect(parsed.publish).toEqual(["*/chat"]);
	expect(parsed.subscribe).toEqual(["**/*.hang"]);
});

test("unknown versions and mixed fields fail closed", () => {
	for (const raw of [
		{ v: 2, root: "demo", publish: ["a"] },
		{ v: 1, root: "demo", put: ["a"] },
		{ v: 1, root: "demo", get: ["a"] },
		{ v: 0, root: "demo", publish: ["a"] },
		{ root: "demo", put: ["a"], publish: ["a"] },
		{ root: "demo", get: ["a"], subscribe: ["a"] },
	]) {
		expect(() => ClaimsSchema.parse(raw)).toThrow();
	}
});

test("v1 rejects invalid patterns and roots", () => {
	expect(() => ClaimsSchema.parse({ v: 1, root: "demo", publish: ["a//b"] })).toThrow();
	expect(() => ClaimsSchema.parse({ v: 1, root: "a*b", publish: ["c"] })).toThrow();
	expect(() => ClaimsSchema.parse({ v: 1, root: "demo", publish: [] })).toThrow();
});

test("v1 authorize - exact residuals below the root", () => {
	const claims: Claims = { v: 1, root: "pid", publish: ["*/chat"], subscribe: ["**/*.hang"] };
	const permissions = authorize(claims, "pid/alice");
	expect(permissions.publish).toEqual(["chat"]);
	expect(permissions.subscribe).toEqual(["**/*.hang"]);
});

test("v1 authorize - exact residuals above the root", () => {
	const claims: Claims = { v: 1, root: "pid", publish: ["*/chat"], subscribe: ["**/*.hang"] };
	const permissions = authorize(claims, "");
	expect(permissions.publish).toEqual(["pid/*/chat"]);
	expect(permissions.subscribe).toEqual(["pid/**/*.hang"]);
});

test("v1 authorize - rejects mismatch and no access", () => {
	const claims: Claims = { v: 1, root: "pid", publish: ["*/chat"] };
	expect(() => authorize(claims, "other")).toThrow(/does not overlap/);
	expect(() => authorize(claims, "pid/bob/extra")).toThrow(/grants no access/);
});

test("v1 scope contains subsets and rejects escapes", () => {
	const scope = { v: 1 as const, root: "pid", publish: ["*/chat"], subscribe: ["**/*.hang"] };
	expect(scopeAllows(scope, { v: 1, root: "pid", publish: ["alice/chat"], subscribe: ["pid/demo.hang"] })).toBe(true);
	expect(scopeAllows(scope, { v: 1, root: "pid", publish: ["alice/chat/extra"] })).toBe(false);
	expect(scopeAllows(scope, { v: 1, root: "pid", subscribe: ["other/demo.msf"] })).toBe(false);
	expect(scopeAllows(scope, { v: 1, root: "other", publish: ["alice/chat"] })).toBe(false);
});

test("scope version must match claims", () => {
	expect(scopeAllows({ root: "pid", put: [""] }, { v: 1, root: "pid", publish: ["a"] })).toBe(false);
	expect(scopeAllows({ v: 1, root: "pid", publish: ["**"] }, { root: "pid", put: ["a"] })).toBe(false);
});
