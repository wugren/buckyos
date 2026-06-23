/**
 * test_user_mgr.ts — DV 测试用例，覆盖 control_panel 的 user_mgr.rs
 *
 * 测试 RPC 方法：
 *   user.*  —— user.list / user.get / user.create / user.update /
 *              user.update_contact / user.change_password /
 *              user.change_state / user.change_type / user.delete
 *   agent.* —— agent.list / agent.get / agent.set_msg_tunnel /
 *              agent.remove_msg_tunnel
 *
 * 通过 deno 直接运行：
 *   deno run --allow-net --allow-read --allow-env \
 *     --unsafely-ignore-certificate-errors test_user_mgr.ts
 */

import { buckyos } from "buckyos";
import { initTestRuntime } from "../test_helpers/buckyos_client.ts";
import {
  fetchUserList,
  fetchUserDetail,
  createUser,
  updateUser,
  updateUserContact,
  fetchUserProfile,
  setUserProfile,
  setUserMsgTunnel,
  removeUserMsgTunnel,
  createUserInvite,
  fetchUserInvite,
  changeUserPassword,
  changeUserState,
  changeUserType,
  deleteUser,
  fetchAgentList,
  fetchAgentDetail,
  createAgent,
  updateAgent,
  deleteAgent,
  fetchAgentProfile,
  setAgentProfile,
  setAgentMsgTunnel,
  removeAgentMsgTunnel,
  type UserDetail,
  type UsersListResponse,
  type AgentsListResponse,
  type UserType,
  type SimpleOkResponse,
} from "../../src/frame/desktop/src/api/user_mgr.ts";

// ---------------------------------------------------------------------------
// Test runner
// ---------------------------------------------------------------------------

type TestResult = {
  name: string;
  ok: boolean;
  durationMs: number;
  error?: string;
};

async function runCase(
  name: string,
  fn: () => Promise<void>,
): Promise<TestResult> {
  const start = Date.now();
  try {
    await fn();
    const ms = Date.now() - start;
    console.log(`  ✓ ${name} (${ms}ms)`);
    return { name, ok: true, durationMs: ms };
  } catch (err) {
    const ms = Date.now() - start;
    const msg = err instanceof Error ? err.message : String(err);
    console.error(`  ✗ ${name} (${ms}ms): ${msg}`);
    return { name, ok: false, durationMs: ms, error: msg };
  }
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`Assertion failed: ${message}`);
}

// Derived from the DV zone config (src/kernel/scheduler/src/system_config_builder.rs)
const DV_DEFAULT_AGENT_ID = "jarvis";
// Fake sha256 hash for change_password — format doesn't matter to the backend,
// it only stores the string verbatim (no login happens after this point).
const FAKE_PW_HASH_B =
  "b2".padEnd(64, "0");

function getEnv(name: string, fallback: string): string {
  const val = Deno.env.get(name);
  return typeof val === "string" && val.trim().length > 0 ? val.trim() : fallback;
}

// Admin credentials used to obtain a sudo grant. The DV zone provisions the
// owner/admin account ("devtest") with the password "bucky2025"; the stored
// hash o8XyToejrbCYou84h/VkF4Tht0BeQQbuX3XKG+8+GQ4= equals
// hashPassword("devtest", "bucky2025") — see
// src/kernel/buckyos-api/src/test_config.rs (ADMIN_PASSWORD_HASH).
const DV_ADMIN_USER = getEnv("BUCKYOS_TEST_ADMIN_USER", "devtest");
const DV_ADMIN_PASSWORD = getEnv("BUCKYOS_TEST_ADMIN_PASSWORD", "bucky2025");
// appid must match the token appid the gateway accepts for the AppClient — the
// test runtime logs in as "buckycli" (see buckyos_client.ts).
const TEST_APP_ID = getEnv("BUCKYOS_TEST_APP_ID", "buckycli");

// ---------------------------------------------------------------------------
// Sudo / local-account auth helpers
//
// Sensitive writes under config/users/* (user.create) require an *elevated*
// sudo session token, not the regular admin login token. The flow mirrors the
// desktop sudo dialog (src/frame/desktop/src/components/sudo.tsx):
//   1. hashPassword(user, pw, nonce) -> verify-hub.sudo_by_password -> sudo token
//   2. call control-panel user.create with that sudo token as the session token
// ---------------------------------------------------------------------------

// The SDK's hashPassword typings are loose (nonce typed as `null`, return as a
// union). The runtime always takes an optional numeric nonce and returns the
// base64 string hash, so narrow it here.
const hashPassword = buckyos.hashPassword as unknown as (
  username: string,
  password: string,
  nonce?: number,
) => string;

// Minimal typed view of the SDK's raw RPC client. We MUST use a raw client
// (not buckyos.getServiceRpcClient): in AppClient mode getServiceRpcClient
// returns a *managed* client that re-injects the runtime's default (non-sudo)
// session token on every call, silently overriding setSessionToken — which
// drops the sudo elevation and makes user.create fail with "No write
// permission". A raw kRPCClient sends exactly the token we pass.
type RawRpcClient = {
  setSeq(seq: number): void;
  call(method: string, params: Record<string, unknown>): Promise<unknown>;
};
const KRpcClient = buckyos.kRPCClient as unknown as new (
  url: string,
  token?: string | null,
  seq?: number,
) => RawRpcClient;

/** Absolute gateway URL for a zone service (AppClient mode has no page origin). */
function serviceUrl(service: string): string {
  const cfg = buckyos.getBuckyOSConfig() as
    | { zoneHost?: string; defaultProtocol?: string }
    | null;
  const host = cfg?.zoneHost;
  if (!host) throw new Error("zoneHost is not configured on the runtime");
  const protocol = cfg?.defaultProtocol ?? "https://";
  return `${protocol}${host}/kapi/${service}`;
}

/** Request an elevated sudo session token from verify-hub via password. */
async function obtainSudoToken(
  username: string,
  password: string,
  appId: string,
): Promise<string> {
  const nonce = Date.now();
  const passwordHash = hashPassword(username, password, nonce);
  // verify-hub login/sudo endpoints are unauthenticated — use a raw client.
  const rpc = new KRpcClient(serviceUrl("verify-hub"), null, nonce);
  const res = (await rpc.call("sudo_by_password", {
    username,
    password: passwordHash,
    appid: appId,
    login_nonce: nonce,
  })) as { session_token?: unknown };
  const token = typeof res?.session_token === "string" ? res.session_token : "";
  if (!token) {
    throw new Error("sudo_by_password did not return a session_token");
  }
  return token;
}

/** Log in a local (password) account via verify-hub. Returns the account info. */
async function loginByPassword(
  username: string,
  password: string,
  appId: string,
): Promise<{
  session_token?: string;
  user_id?: string;
  user_info?: { user_id?: string; user_type?: string; show_name?: string };
}> {
  const nonce = Date.now();
  const passwordHash = hashPassword(username, password, nonce);
  const rpc = new KRpcClient(serviceUrl("verify-hub"), null, nonce);
  return (await rpc.call("login_by_password", {
    type: "password",
    username,
    password: passwordHash,
    appid: appId,
    login_nonce: nonce,
  })) as Awaited<ReturnType<typeof loginByPassword>>;
}

/**
 * Call control-panel `user.create` with an explicit session token (the sudo
 * grant). Mirrors the param mapping of the typed `createUser` helper, but lets
 * us drive the auth token directly via a raw client (see RawRpcClient note).
 */
async function createUserWithToken(
  token: string,
  input: {
    userId: string;
    passwordHash: string;
    showName?: string;
    userType?: Exclude<UserType, "root">;
    allowPasswordChange?: boolean;
  },
): Promise<{ data: SimpleOkResponse | null; error: unknown }> {
  try {
    const rpc = new KRpcClient(serviceUrl("control-panel"), token);
    const params: Record<string, unknown> = {
      user_id: input.userId,
      password_hash: input.passwordHash,
    };
    if (input.showName !== undefined) params.show_name = input.showName;
    if (input.userType !== undefined) params.user_type = input.userType;
    if (input.allowPasswordChange !== undefined) {
      params.allow_password_change = input.allowPasswordChange;
    }
    const result = (await rpc.call("user.create", params)) as SimpleOkResponse;
    if (!result || typeof result !== "object") {
      throw new Error("Invalid user.create response");
    }
    return { data: result, error: null };
  } catch (error) {
    return { data: null, error };
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log("=== test_user_mgr: user_mgr.rs DV tests ===\n");

  // 1. Initialize as AppClient (must run as an admin account, e.g. devtest)
  const { userId: callerUserId, ownerUserId, zoneHost } =
    await initTestRuntime();
  console.log(
    `zone: ${zoneHost}, caller: ${callerUserId}, owner: ${ownerUserId}\n`,
  );

  const results: TestResult[] = [];

  // Unique username for the write-cycle tests. Keeps reruns isolated even
  // though user.delete is a soft-delete (state="deleted") in this backend.
  const testUserId = `dvtest${Date.now()}`;

  // -----------------------------------------------------------------------
  // Read-only: user.list
  // -----------------------------------------------------------------------
  console.log("[user.list]");

  let userListAtStart: UsersListResponse | null = null;

  results.push(
    await runCase("user.list returns valid response", async () => {
      const { data, error } = await fetchUserList();
      assert(!error, `fetchUserList should not error: ${error}`);
      assert(data !== null, "data should not be null");
      userListAtStart = data!;
      assert(typeof data!.total === "number", "total should be number");
      assert(Array.isArray(data!.users), "users should be array");
      assert(
        data!.total === data!.users.length,
        `total (${data!.total}) should match users.length (${data!.users.length})`,
      );
    }),
  );

  results.push(
    await runCase("user.list contains root-tier account", async () => {
      assert(userListAtStart !== null, "userListAtStart should be populated");
      // DV always provisions at least a Root account on first boot.
      assert(
        userListAtStart!.users.length >= 1,
        `expected >= 1 default user, got ${userListAtStart!.users.length}`,
      );
      const roots = userListAtStart!.users.filter(
        (u) => u.user_type === "root",
      );
      assert(roots.length >= 1, "should have at least one root account");
    }),
  );

  results.push(
    await runCase("user.list entries have required fields", async () => {
      assert(userListAtStart !== null, "userListAtStart should be populated");
      for (const u of userListAtStart!.users) {
        assert(
          typeof u.user_id === "string" && u.user_id.length > 0,
          "user_id should be non-empty string",
        );
        assert(
          typeof u.show_name === "string",
          `user '${u.user_id}' show_name should be string`,
        );
        assert(
          typeof u.user_type === "string",
          `user '${u.user_id}' user_type should be string`,
        );
        assert(
          typeof u.state === "string",
          `user '${u.user_id}' state should be string`,
        );
      }
    }),
  );

  // -----------------------------------------------------------------------
  // Read-only: user.get
  // -----------------------------------------------------------------------
  console.log("\n[user.get]");

  results.push(
    await runCase("user.get (no user_id) returns caller detail", async () => {
      const { data, error } = await fetchUserDetail();
      assert(!error, `fetchUserDetail should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(
        typeof data!.user_id === "string",
        "user_id should be string",
      );
      assert(
        typeof data!.show_name === "string",
        "show_name should be string",
      );
      assert(
        typeof data!.state === "string",
        "state should be string",
      );
      assert(
        typeof data!.res_pool_id === "string",
        "res_pool_id should be string",
      );
      // Calling yourself should include contact
      if (data!.contact !== undefined) {
        assert(
          typeof data!.contact === "object" && data!.contact !== null,
          "contact should be object when returned",
        );
      }
    }),
  );

  results.push(
    await runCase(
      "user.get with explicit user_id returns detail",
      async () => {
        const { data, error } = await fetchUserDetail({ userId: callerUserId });
        assert(!error, `fetchUserDetail('${callerUserId}') should not error: ${error}`);
        assert(data !== null, "data should not be null");
        // Password must never be returned
        assert(
          !("password" in (data as unknown as Record<string, unknown>)),
          "user.get response must not expose password field",
        );
      },
    ),
  );

  results.push(
    await runCase("user.get for non-existent user returns error", async () => {
      const { data, error } = await fetchUserDetail({
        userId: "nonexistent_user_xyz_12345",
      });
      assert(
        error !== null || data === null,
        "request should fail for non-existent user",
      );
    }),
  );

  // -----------------------------------------------------------------------
  // Write cycle: create → update → contact → password → state → type → delete
  // -----------------------------------------------------------------------
  console.log(`\n[user write cycle: ${testUserId}]`);

  // The new account is a *local* (password) account — created with a username
  // and a password hash, logged in via verify-hub.login_by_password. It is not
  // an account backed by an independent BNS identity.
  //
  // IMPORTANT: the stored password hash must be hashPassword(user, pw) with NO
  // nonce (the "org" hash). verify-hub re-salts it with the per-login nonce
  // (src/kernel/verify_hub/src/main.rs::validate_password_login), so a fake
  // hash here would store fine but make login impossible.
  const NEW_USER_PASSWORD = "DvTest-Pass-2025";
  const newUserPasswordHash = hashPassword(testUserId, NEW_USER_PASSWORD);

  let sudoToken: string | null = null;

  results.push(
    await runCase("sudo: admin obtains an elevated sudo token", async () => {
      sudoToken = await obtainSudoToken(
        DV_ADMIN_USER,
        DV_ADMIN_PASSWORD,
        TEST_APP_ID,
      );
      assert(
        typeof sudoToken === "string" && sudoToken.length > 0,
        "sudo token should be a non-empty string",
      );
    }),
  );

  results.push(
    await runCase("user.create is rejected without sudo", async () => {
      // Uses the regular (non-elevated) admin session token. Sensitive writes
      // under config/users/* require sudo, so this must fail. A distinct
      // user_id keeps this isolated from the real create below.
      const { data, error } = await createUser({
        userId: `${testUserId}nosudo`,
        passwordHash: newUserPasswordHash,
        showName: "DV Test User (no sudo)",
        userType: "user",
      });
      assert(
        error !== null || data === null,
        "create without sudo must be rejected",
      );
    }),
  );

  results.push(
    await runCase("user.create with sudo creates a new local user", async () => {
      assert(sudoToken !== null, "sudoToken should have been granted");
      const { data, error } = await createUserWithToken(sudoToken!, {
        userId: testUserId,
        passwordHash: newUserPasswordHash,
        showName: `DV Test User ${testUserId}`,
        userType: "user",
      });
      assert(!error, `createUser (sudo) should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        data!.user_id === testUserId,
        `user_id should be '${testUserId}', got '${data!.user_id}'`,
      );
    }),
  );

  results.push(
    await runCase(
      "new local user can log in with username + password",
      async () => {
        const res = await loginByPassword(
          testUserId,
          NEW_USER_PASSWORD,
          TEST_APP_ID,
        );
        assert(
          typeof res?.session_token === "string" &&
            res.session_token.length > 0,
          "login should return a non-empty session_token",
        );
        const loggedInUserId = res.user_info?.user_id ?? res.user_id;
        assert(
          loggedInUserId === testUserId,
          `logged-in user_id should be '${testUserId}', got '${loggedInUserId}'`,
        );
      },
    ),
  );

  results.push(
    await runCase("user.create rejects duplicate user_id (with sudo)", async () => {
      assert(sudoToken !== null, "sudoToken should have been granted");
      const { data, error } = await createUserWithToken(sudoToken!, {
        userId: testUserId,
        passwordHash: newUserPasswordHash,
      });
      assert(
        error !== null || data === null,
        "duplicate create should fail",
      );
    }),
  );

  results.push(
    await runCase("user.create rejects reserved username (with sudo)", async () => {
      assert(sudoToken !== null, "sudoToken should have been granted");
      const { data, error } = await createUserWithToken(sudoToken!, {
        userId: "root",
        passwordHash: newUserPasswordHash,
      });
      assert(
        error !== null || data === null,
        "create with reserved name 'root' should fail",
      );
    }),
  );

  results.push(
    await runCase("user.list now contains the new user", async () => {
      const { data, error } = await fetchUserList();
      assert(!error, `fetchUserList should not error: ${error}`);
      assert(data !== null, "data should not be null");
      const found = data!.users.find((u) => u.user_id === testUserId);
      assert(!!found, `new user '${testUserId}' should appear in user.list`);
      assert(
        found!.user_type === "user",
        `new user type should be 'user', got '${found!.user_type}'`,
      );
      assert(
        found!.state === "active",
        `new user state should be 'active', got '${found!.state}'`,
      );
    }),
  );

  let detailAfterCreate: UserDetail | null = null;
  results.push(
    await runCase("user.get returns the new user's detail", async () => {
      const { data, error } = await fetchUserDetail({ userId: testUserId });
      assert(!error, `fetchUserDetail('${testUserId}') should not error: ${error}`);
      assert(data !== null, "data should not be null");
      detailAfterCreate = data!;
      assert(
        data!.user_id === testUserId,
        `user_id mismatch: '${data!.user_id}' vs '${testUserId}'`,
      );
      assert(
        data!.show_name === `DV Test User ${testUserId}`,
        `show_name mismatch: '${data!.show_name}'`,
      );
      assert(data!.state === "active", `state should be 'active'`);
    }),
  );

  results.push(
    await runCase("user.update changes show_name", async () => {
      const newShowName = `Renamed ${testUserId}`;
      const { data, error } = await updateUser({
        userId: testUserId,
        showName: newShowName,
      });
      assert(!error, `updateUser should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");

      const { data: refreshed, error: refreshErr } = await fetchUserDetail({
        userId: testUserId,
      });
      assert(!refreshErr, `fetchUserDetail should not error: ${refreshErr}`);
      assert(refreshed !== null, "refreshed detail should not be null");
      assert(
        refreshed!.show_name === newShowName,
        `show_name should be '${newShowName}', got '${refreshed!.show_name}'`,
      );
    }),
  );

  results.push(
    await runCase("user.update_contact writes contact settings", async () => {
      const { data, error } = await updateUserContact({
        userId: testUserId,
        did: `did:bns:${testUserId}`,
        note: "created by test_user_mgr",
        groups: ["dv-test"],
        tags: ["automation", "temporary"],
      });
      assert(!error, `updateUserContact should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(!!data!.contact, "response should echo contact");
      assert(
        data!.contact!.did === `did:bns:${testUserId}`,
        `contact.did mismatch: '${data!.contact!.did}'`,
      );
      assert(
        data!.contact!.note === "created by test_user_mgr",
        `contact.note mismatch`,
      );
      assert(
        Array.isArray(data!.contact!.groups) &&
          data!.contact!.groups!.includes("dv-test"),
        "contact.groups should contain 'dv-test'",
      );
      assert(
        Array.isArray(data!.contact!.tags) &&
          data!.contact!.tags!.length === 2,
        "contact.tags should have 2 entries",
      );
    }),
  );

  results.push(
    await runCase("user.get reflects contact update (self or admin)", async () => {
      const { data, error } = await fetchUserDetail({ userId: testUserId });
      assert(!error, `fetchUserDetail should not error: ${error}`);
      assert(data !== null, "data should not be null");
      // Caller is admin, so contact must be included.
      assert(!!data!.contact, "contact should be visible to admin caller");
      assert(
        data!.contact!.did === `did:bns:${testUserId}`,
        "contact.did should persist",
      );
    }),
  );

  // -----------------------------------------------------------------------
  // user.profile.get / user.profile.set
  // -----------------------------------------------------------------------
  console.log(`\n[user.profile.get / user.profile.set: ${testUserId}]`);

  results.push(
    await runCase("user.profile.set writes local profile fields", async () => {
      const { data, error } = await setUserProfile({
        userId: testUserId,
        displayName: `DV ${testUserId}`,
        title: "QA Bot",
        bio: "created by test_user_mgr",
        location: "DV Zone",
        website: "https://buckyos.io",
      });
      assert(!error, `setUserProfile should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
    }),
  );

  results.push(
    await runCase("user.profile.get returns the written profile", async () => {
      const { data, error } = await fetchUserProfile(testUserId);
      assert(!error, `fetchUserProfile should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(
        data!.user_id === testUserId,
        `user_id should be '${testUserId}', got '${data!.user_id}'`,
      );
      const profile = (data!.profile ?? {}) as Record<string, unknown>;
      // The merged profile (local + DID) must reflect the local write.
      assert(
        profile.title === "QA Bot",
        `profile.title should be 'QA Bot', got '${String(profile.title)}'`,
      );
      assert(
        profile.bio === "created by test_user_mgr",
        `profile.bio should round-trip, got '${String(profile.bio)}'`,
      );
    }),
  );

  // -----------------------------------------------------------------------
  // user.set_msg_tunnel / user.remove_msg_tunnel
  // -----------------------------------------------------------------------
  console.log(
    `\n[user.set_msg_tunnel / user.remove_msg_tunnel: ${testUserId}]`,
  );

  const userPlatform = `dvtest_user_${Date.now()}`;

  results.push(
    await runCase("user.set_msg_tunnel adds a binding", async () => {
      const { data, error } = await setUserMsgTunnel({
        userId: testUserId,
        platform: userPlatform,
        accountId: "dv-user-account",
        displayId: "DV User Display",
        tunnelId: "dv-user-tunnel",
        meta: { source: "test_user_mgr" },
      });
      assert(!error, `setUserMsgTunnel should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        data!.platform === userPlatform,
        "platform should round-trip",
      );
      assert(
        typeof data!.total_bindings === "number" && data!.total_bindings! >= 1,
        "total_bindings should be >= 1",
      );
    }),
  );

  results.push(
    await runCase(
      "user.set_msg_tunnel binding is visible via user.get contact",
      async () => {
        const { data, error } = await fetchUserDetail({ userId: testUserId });
        assert(!error, `fetchUserDetail should not error: ${error}`);
        assert(data !== null, "data should not be null");
        const bindings = data!.contact?.bindings ?? [];
        const found = bindings.find((b) => b.platform === userPlatform);
        assert(
          !!found,
          `binding for '${userPlatform}' should appear in contact.bindings`,
        );
        assert(
          found!.account_id === "dv-user-account",
          "binding account_id should round-trip",
        );
      },
    ),
  );

  results.push(
    await runCase("user.remove_msg_tunnel removes the binding", async () => {
      const { data, error } = await removeUserMsgTunnel({
        userId: testUserId,
        platform: userPlatform,
      });
      assert(!error, `removeUserMsgTunnel should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        data!.platform === userPlatform,
        "platform should round-trip",
      );
    }),
  );

  results.push(
    await runCase("user.change_password updates password hash", async () => {
      const { data, error } = await changeUserPassword({
        userId: testUserId,
        newPasswordHash: FAKE_PW_HASH_B,
      });
      assert(!error, `changeUserPassword should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
    }),
  );

  results.push(
    await runCase("user.change_password rejects empty hash", async () => {
      const { data, error } = await changeUserPassword({
        userId: testUserId,
        newPasswordHash: "",
      });
      assert(
        error !== null || data === null,
        "empty password hash should be rejected",
      );
    }),
  );

  results.push(
    await runCase("user.change_state sets suspended:reason", async () => {
      const { data, error } = await changeUserState({
        userId: testUserId,
        state: "suspended:dv-test",
      });
      assert(!error, `changeUserState should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");

      const { data: refreshed } = await fetchUserDetail({ userId: testUserId });
      assert(refreshed !== null, "refreshed detail should not be null");
      assert(
        refreshed!.state === "suspended:dv-test",
        `state should be 'suspended:dv-test', got '${refreshed!.state}'`,
      );
    }),
  );

  results.push(
    await runCase("user.change_state can re-activate the user", async () => {
      const { data, error } = await changeUserState({
        userId: testUserId,
        state: "active",
      });
      assert(!error, `changeUserState(active) should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");

      const { data: refreshed } = await fetchUserDetail({ userId: testUserId });
      assert(refreshed !== null, "refreshed detail should not be null");
      assert(
        refreshed!.state === "active",
        `state should be 'active', got '${refreshed!.state}'`,
      );
    }),
  );

  results.push(
    await runCase("user.change_type promotes/demotes the user", async () => {
      const { data, error } = await changeUserType({
        userId: testUserId,
        userType: "limited",
      });
      assert(!error, `changeUserType should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");

      const { data: refreshed } = await fetchUserDetail({ userId: testUserId });
      assert(refreshed !== null, "refreshed detail should not be null");
      assert(
        refreshed!.user_type === "limited",
        `user_type should be 'limited', got '${refreshed!.user_type}'`,
      );
    }),
  );

  results.push(
    await runCase("user.change_type rejects promotion to root", async () => {
      const { data, error } = await changeUserType({
        userId: testUserId,
        // Bypass the compile-time exclusion for negative testing
        userType: "root" as unknown as "user",
      });
      assert(
        error !== null || data === null,
        "promotion to root must be rejected",
      );
    }),
  );

  // -----------------------------------------------------------------------
  // Protection tests — root must stay untouched
  // -----------------------------------------------------------------------
  console.log("\n[protection]");

  results.push(
    await runCase("user.delete rejects deleting 'root'", async () => {
      const { data, error } = await deleteUser("root");
      assert(
        error !== null || data === null,
        "deleting root must be rejected",
      );
    }),
  );

  results.push(
    await runCase("user.delete rejects deleting self", async () => {
      const { data, error } = await deleteUser(callerUserId);
      assert(
        error !== null || data === null,
        "deleting self must be rejected",
      );
    }),
  );

  results.push(
    await runCase(
      "user.change_state rejects suspending root",
      async () => {
        const { data, error } = await changeUserState({
          userId: "root",
          state: "suspended:should-fail",
        });
        assert(
          error !== null || data === null,
          "suspending root must be rejected",
        );
      },
    ),
  );

  // -----------------------------------------------------------------------
  // Soft-delete — do this last so nothing else depends on the user
  // -----------------------------------------------------------------------
  console.log("\n[user.delete]");

  results.push(
    await runCase("user.delete soft-deletes the test user", async () => {
      const { data, error } = await deleteUser(testUserId);
      assert(!error, `deleteUser should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");

      const { data: refreshed } = await fetchUserDetail({ userId: testUserId });
      assert(
        refreshed !== null,
        "soft-deleted user should still be retrievable",
      );
      assert(
        refreshed!.state === "deleted",
        `state should be 'deleted', got '${refreshed!.state}'`,
      );
    }),
  );

  // -----------------------------------------------------------------------
  // user.invite.create / user.invite.get
  // invite.accept is intentionally not exercised here: it requires a valid
  // owner_config DID document that can only be produced by a real client
  // keypair, which is out of scope for this RPC-surface smoke test.
  // -----------------------------------------------------------------------
  console.log("\n[user.invite.create / user.invite.get]");

  const inviteTargetUserId = `dvinvite${Date.now()}`;
  let createdInviteId: string | null = null;

  results.push(
    await runCase("user.invite.create generates a pending invite", async () => {
      const { data, error } = await createUserInvite({
        userId: inviteTargetUserId,
        showName: `Invited ${inviteTargetUserId}`,
        userType: "user",
        ttlSecs: 3600,
        groups: ["dv-test"],
      });
      assert(!error, `createUserInvite should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(!!data!.invite, "response should carry an invite record");
      assert(
        typeof data!.invite.invite_id === "string" &&
          data!.invite.invite_id.length > 0,
        "invite_id should be a non-empty string",
      );
      createdInviteId = data!.invite.invite_id;
      assert(
        data!.invite.state === "pending",
        `invite state should be 'pending', got '${data!.invite.state}'`,
      );
      assert(
        data!.invite.target_user_id === inviteTargetUserId,
        `invite target_user_id should be '${inviteTargetUserId}'`,
      );
      assert(
        typeof data!.invite_url === "string" && data!.invite_url!.length > 0,
        "response should include an invite_url",
      );
    }),
  );

  results.push(
    await runCase("user.invite.get reads the invite back (public)", async () => {
      assert(createdInviteId !== null, "createdInviteId should be populated");
      const { data, error } = await fetchUserInvite(createdInviteId!);
      assert(!error, `fetchUserInvite should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(
        data!.invite.invite_id === createdInviteId,
        "invite_id should round-trip",
      );
      assert(
        data!.invite.state === "pending",
        `invite state should be 'pending', got '${data!.invite.state}'`,
      );
      // Public invite view must carry zone routing info for the accept page.
      assert(
        typeof data!.zone_host === "string" && data!.zone_host!.length > 0,
        "invite response should include zone_host",
      );
    }),
  );

  results.push(
    await runCase("user.invite.get fails for unknown invite_id", async () => {
      const { data, error } = await fetchUserInvite("nonexistent_invite_xyz");
      assert(
        error !== null || data === null,
        "unknown invite_id should fail",
      );
    }),
  );

  // Clean up the pending user that invite.create pre-provisioned.
  results.push(
    await runCase("cleanup: soft-delete the invited pending user", async () => {
      const { data, error } = await deleteUser(inviteTargetUserId);
      assert(!error, `deleteUser should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
    }),
  );

  // -----------------------------------------------------------------------
  // Read-only: agent.list / agent.get
  // -----------------------------------------------------------------------
  console.log("\n[agent.list / agent.get]");

  let agentList: AgentsListResponse | null = null;

  results.push(
    await runCase("agent.list returns valid response", async () => {
      const { data, error } = await fetchAgentList();
      assert(!error, `fetchAgentList should not error: ${error}`);
      assert(data !== null, "data should not be null");
      agentList = data!;
      assert(typeof data!.total === "number", "total should be number");
      assert(Array.isArray(data!.agents), "agents should be array");
      assert(
        data!.total === data!.agents.length,
        `total (${data!.total}) should match agents.length (${data!.agents.length})`,
      );
    }),
  );

  results.push(
    await runCase(
      `agent.list includes DV default agent (${DV_DEFAULT_AGENT_ID})`,
      async () => {
        assert(agentList !== null, "agentList should be populated");
        const found = agentList!.agents.find(
          (a) => a.agent_id === DV_DEFAULT_AGENT_ID,
        );
        assert(
          !!found,
          `DV default agent '${DV_DEFAULT_AGENT_ID}' should be in agent.list`,
        );
      },
    ),
  );

  results.push(
    await runCase(
      `agent.get returns doc for ${DV_DEFAULT_AGENT_ID}`,
      async () => {
        const { data, error } = await fetchAgentDetail(DV_DEFAULT_AGENT_ID);
        assert(!error, `fetchAgentDetail should not error: ${error}`);
        assert(data !== null, "data should not be null");
        assert(
          data!.agent_id === DV_DEFAULT_AGENT_ID,
          `agent_id should be '${DV_DEFAULT_AGENT_ID}'`,
        );
        // jarvis has an `id` (DID) field in its doc
        assert(
          typeof (data as Record<string, unknown>).id === "string",
          "agent doc should carry an 'id' (DID) field",
        );
        // settings is optional but, when present, must be an object
        if (data!.settings !== undefined) {
          assert(
            typeof data!.settings === "object" && data!.settings !== null,
            "settings should be object when present",
          );
        }
      },
    ),
  );

  results.push(
    await runCase("agent.get for non-existent agent returns error", async () => {
      const { data, error } = await fetchAgentDetail("nonexistent_agent_xyz");
      assert(
        error !== null || data === null,
        "request should fail for non-existent agent",
      );
    }),
  );

  // -----------------------------------------------------------------------
  // Agent write cycle: create → get → update → profile → delete
  // Agent identity management is Admin-only (Agents are a Zone-level resource).
  // -----------------------------------------------------------------------
  const testAgentId = `dvagent${Date.now()}`;
  console.log(`\n[agent write cycle: ${testAgentId}]`);

  results.push(
    await runCase("agent.create creates a new agent identity", async () => {
      const { data, error } = await createAgent({
        agentId: testAgentId,
        displayName: `DV Agent ${testAgentId}`,
        ownerUserId: callerUserId,
        description: "created by test_user_mgr",
      });
      assert(!error, `createAgent should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        data!.agent_id === testAgentId,
        `agent_id should be '${testAgentId}', got '${data!.agent_id}'`,
      );
    }),
  );

  results.push(
    await runCase("agent.create rejects duplicate agent_id", async () => {
      const { data, error } = await createAgent({ agentId: testAgentId });
      assert(
        error !== null || data === null,
        "duplicate agent create should fail",
      );
    }),
  );

  results.push(
    await runCase("agent.list now contains the new agent", async () => {
      const { data, error } = await fetchAgentList();
      assert(!error, `fetchAgentList should not error: ${error}`);
      assert(data !== null, "data should not be null");
      const found = data!.agents.find((a) => a.agent_id === testAgentId);
      assert(!!found, `new agent '${testAgentId}' should appear in agent.list`);
    }),
  );

  results.push(
    await runCase("agent.get returns the new agent's detail", async () => {
      const { data, error } = await fetchAgentDetail(testAgentId);
      assert(!error, `fetchAgentDetail should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(
        data!.agent_id === testAgentId,
        `agent_id should be '${testAgentId}'`,
      );
    }),
  );

  results.push(
    await runCase("agent.update changes display_name", async () => {
      const { data, error } = await updateAgent({
        agentId: testAgentId,
        displayName: `Renamed ${testAgentId}`,
      });
      assert(!error, `updateAgent should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
    }),
  );

  results.push(
    await runCase("agent.profile.set then agent.profile.get round-trips", async () => {
      const { data: setData, error: setErr } = await setAgentProfile({
        agentId: testAgentId,
        profile: { title: "DV Test Agent", bio: "round-trip check" },
      });
      assert(!setErr, `setAgentProfile should not error: ${setErr}`);
      assert(setData !== null, "set data should not be null");
      assert(setData!.ok === true, "ok should be true");

      const { data, error } = await fetchAgentProfile(testAgentId);
      assert(!error, `fetchAgentProfile should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(
        data!.agent_id === testAgentId,
        "profile agent_id should round-trip",
      );
      const profile = (data!.profile ?? {}) as Record<string, unknown>;
      assert(
        profile.title === "DV Test Agent",
        `profile.title should round-trip, got '${String(profile.title)}'`,
      );
    }),
  );

  results.push(
    await runCase("agent.delete soft-deletes the test agent", async () => {
      const { data, error } = await deleteAgent(testAgentId);
      assert(!error, `deleteAgent should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        (data as Record<string, unknown>).state === "deleted",
        "delete response should report state 'deleted'",
      );

      // Soft-delete keeps the doc; agent.get still resolves but settings.state
      // is now 'deleted'.
      const { data: after, error: afterErr } = await fetchAgentDetail(
        testAgentId,
      );
      assert(!afterErr, `fetchAgentDetail should not error: ${afterErr}`);
      assert(after !== null, "soft-deleted agent should still be retrievable");
      const settings = (after!.settings ?? {}) as Record<string, unknown>;
      assert(
        settings.state === "deleted",
        `settings.state should be 'deleted', got '${String(settings.state)}'`,
      );
    }),
  );

  // -----------------------------------------------------------------------
  // agent.set_msg_tunnel / agent.remove_msg_tunnel
  // Use a throwaway platform name unique to this run so we don't clobber
  // any real-world binding.
  // -----------------------------------------------------------------------
  console.log("\n[agent.set_msg_tunnel / agent.remove_msg_tunnel]");

  const testPlatform = `dvtest_${Date.now()}`;

  results.push(
    await runCase("agent.set_msg_tunnel adds a new binding", async () => {
      const { data, error } = await setAgentMsgTunnel({
        agentId: DV_DEFAULT_AGENT_ID,
        platform: testPlatform,
        accountId: "dv-test-account",
        displayId: "DV Test Display",
        tunnelId: "dv-test-tunnel",
        meta: { source: "test_user_mgr" },
      });
      assert(!error, `setAgentMsgTunnel should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        data!.agent_id === DV_DEFAULT_AGENT_ID,
        "agent_id should round-trip",
      );
      assert(
        data!.platform === testPlatform,
        "platform should round-trip",
      );
      assert(
        typeof data!.total_bindings === "number" && data!.total_bindings! >= 1,
        "total_bindings should be >= 1",
      );
    }),
  );

  results.push(
    await runCase("agent.set_msg_tunnel is idempotent per platform", async () => {
      // Re-setting the same platform should replace, not duplicate.
      const { data, error } = await setAgentMsgTunnel({
        agentId: DV_DEFAULT_AGENT_ID,
        platform: testPlatform,
        accountId: "dv-test-account-v2",
      });
      assert(!error, `setAgentMsgTunnel (update) should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
    }),
  );

  results.push(
    await runCase("agent.remove_msg_tunnel removes the binding", async () => {
      const { data, error } = await removeAgentMsgTunnel({
        agentId: DV_DEFAULT_AGENT_ID,
        platform: testPlatform,
      });
      assert(!error, `removeAgentMsgTunnel should not error: ${error}`);
      assert(data !== null, "data should not be null");
      assert(data!.ok === true, "ok should be true");
      assert(
        data!.platform === testPlatform,
        "platform should round-trip",
      );
    }),
  );

  results.push(
    await runCase(
      "agent.remove_msg_tunnel fails for already-removed platform",
      async () => {
        const { data, error } = await removeAgentMsgTunnel({
          agentId: DV_DEFAULT_AGENT_ID,
          platform: testPlatform,
        });
        assert(
          error !== null || data === null,
          "removing an already-removed platform should fail",
        );
      },
    ),
  );

  // -----------------------------------------------------------------------
  // Summary
  // -----------------------------------------------------------------------
  console.log("\n=== Summary ===");
  const passed = results.filter((r) => r.ok).length;
  const failed = results.filter((r) => !r.ok).length;
  const totalMs = results.reduce((sum, r) => sum + r.durationMs, 0);
  console.log(
    `${passed} passed, ${failed} failed, ${results.length} total (${totalMs}ms)`,
  );

  if (failed > 0) {
    console.log("\nFailed tests:");
    for (const r of results.filter((r) => !r.ok)) {
      console.log(`  - ${r.name}: ${r.error}`);
    }
    Deno.exit(1);
  }

  console.log("\nAll tests passed!");
}

main().catch((err) => {
  console.error("Fatal error:", err);
  Deno.exit(2);
});
