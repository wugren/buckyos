import { initTestRuntime } from "../../test_helpers/buckyos_client.ts";

type JsonPrimitive = string | number | boolean | null;
type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
type JsonObject = { [key: string]: JsonValue };

type RpcClient = {
  call: (method: string, params: Record<string, unknown>) => Promise<unknown>;
};

type TaskRecord = {
  id: number;
  name: string;
  task_type: string;
  runner: string;
  status: string;
  root_id: string;
  data?: JsonValue;
};

type StreamReader = {
  readFrame: (timeoutMs: number) => Promise<JsonObject>;
  close: () => void;
};

const assert = {
  ok(value: unknown, message?: string): void {
    if (!value) throw new Error(message ?? "assertion failed");
  },
  equal(actual: unknown, expected: unknown, message?: string): void {
    if (actual !== expected) {
      throw new Error(
        message ?? `expected ${String(expected)}, got ${String(actual)}`,
      );
    }
  },
};

function getEnv(name: string, fallback?: string): string | undefined {
  const value = Deno.env.get(name);
  if (typeof value === "string" && value.trim().length > 0) {
    return value.trim();
  }
  return fallback;
}

async function prepareAppClientConfig(): Promise<void> {
  const sourceDir = getEnv(
    "BUCKYOS_TEST_APP_CLIENT_DIR",
    "/opt/buckyos/etc/.buckycli",
  )!;
  const tempDir = await Deno.makeTempDir({ prefix: "buckyos-appclient-" });

  await Deno.copyFile(
    `${sourceDir}/user_private_key.pem`,
    `${tempDir}/user_private_key.pem`,
  );

  const userConfig = JSON.parse(
    await Deno.readTextFile(`${sourceDir}/user_config.json`),
  ) as JsonObject;
  if (Array.isArray(userConfig["@context"])) {
    const contexts = userConfig["@context"].filter((item) =>
      typeof item === "string"
    );
    userConfig["@context"] = contexts.at(-1) ??
      "https://buckyos.org/ns/owner/v1";
  }
  await Deno.writeTextFile(
    `${tempDir}/user_config.json`,
    `${JSON.stringify(userConfig, null, 2)}\n`,
  );

  try {
    await Deno.copyFile(
      `${sourceDir}/zone_config.json`,
      `${tempDir}/zone_config.json`,
    );
  } catch {
    // zone_config.json is not required by the WebSDK private key loader.
  }

  Deno.env.set("BUCKYOS_TEST_APP_CLIENT_DIR", tempDir);
}

function asObject(value: unknown, label: string): JsonObject {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} is not an object: ${JSON.stringify(value)}`);
  }
  return value as JsonObject;
}

function asTasks(value: unknown): TaskRecord[] {
  const object = asObject(value, "task-manager response");
  const tasks = object.tasks;
  if (!Array.isArray(tasks)) {
    throw new Error(`expected tasks array, got ${JSON.stringify(value)}`);
  }
  return tasks as TaskRecord[];
}

async function rpcCall<T>(
  rpc: RpcClient,
  method: string,
  params: Record<string, unknown>,
): Promise<T> {
  return await rpc.call(method, params) as T;
}

async function openKeventStream(
  gatewayBaseUrl: string,
  pattern: string,
): Promise<StreamReader> {
  const controller = new AbortController();
  const response = await fetch(`${gatewayBaseUrl}/kapi/kevent/stream`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      patterns: [pattern],
      keepalive_ms: 1000,
    }),
    signal: controller.signal,
  });

  if (!response.ok || !response.body) {
    const text = await response.text().catch(() => "");
    throw new Error(
      `kevent stream failed: status=${response.status}, body=${text}`,
    );
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  async function readFrame(timeoutMs: number): Promise<JsonObject> {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const newline = buffer.indexOf("\n");
      if (newline >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (line.length === 0) continue;
        return asObject(JSON.parse(line), "kevent stream frame");
      }

      const remaining = deadline - Date.now();
      const readPromise = reader.read();
      const timeoutPromise = new Promise<ReadableStreamReadResult<Uint8Array>>(
        (_resolve, reject) =>
          setTimeout(
            () => reject(new Error("kevent stream read timeout")),
            remaining,
          ),
      );
      const chunk = await Promise.race([readPromise, timeoutPromise]);
      if (chunk.done) throw new Error("kevent stream closed");
      buffer += decoder.decode(chunk.value, { stream: true });
    }
    throw new Error("kevent stream read timeout");
  }

  return {
    readFrame,
    close(): void {
      controller.abort();
      reader.cancel().catch(() => {});
    },
  };
}

async function createTask(
  taskMgr: RpcClient,
  args: {
    name: string;
    taskType: string;
    runner: string;
    userId: string;
    appId: string;
    sessionId: string;
    data: JsonObject;
  },
): Promise<TaskRecord> {
  const result = await rpcCall<JsonObject>(taskMgr, "create_task", {
    name: args.name,
    task_type: args.taskType,
    runner: args.runner,
    user_id: args.userId,
    app_id: args.appId,
    session_id: args.sessionId,
    data: args.data,
  });
  const resultTask = result.task === undefined || result.task === null
    ? null
    : asObject(result.task, "created task");
  const id = resultTask?.id ?? result.task_id ?? result.id;
  if (typeof id !== "number") {
    throw new Error(`create_task did not return task id: ${JSON.stringify(result)}`);
  }
  const fetched = await rpcCall<JsonObject>(taskMgr, "get_task", { id });
  return asObject(fetched.task ?? fetched, "created task") as unknown as TaskRecord;
}

async function deleteTask(taskMgr: RpcClient, id: number): Promise<void> {
  try {
    await taskMgr.call("delete_task", { id });
  } catch (error) {
    console.log("[warn] cleanup delete_task failed", id, String(error));
  }
}

async function listRunnerTasks(
  taskMgr: RpcClient,
  runner: string,
  taskType: string,
  sourceUserId: string,
  sourceAppId: string,
): Promise<TaskRecord[]> {
  return asTasks(
    await taskMgr.call("list_tasks", {
      runner,
      task_type: taskType,
      status: "Pending",
      source_user_id: sourceUserId,
      source_app_id: sourceAppId,
    }),
  );
}

async function waitForTaskReadyEvent(
  stream: StreamReader,
  expectedEventId: string,
  expectedTaskId: number,
): Promise<JsonObject> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const frame = await stream.readFrame(Math.max(1, deadline - Date.now()));
    if (frame.type !== "event") continue;
    const event = asObject(frame.event, "kevent event");
    if (event.eventid !== expectedEventId) continue;
    const data = asObject(event.data, "task_ready payload");
    if (data.task_id === expectedTaskId) return data;
  }
  throw new Error(
    `did not receive ${expectedEventId} for task ${expectedTaskId}`,
  );
}

async function run(): Promise<void> {
  await prepareAppClientConfig();
  const { buckyos, userId, ownerUserId, zoneHost } = await initTestRuntime();
  const appId = getEnv("BUCKYOS_TEST_APP_ID", "buckycli")!;
  const gatewayBaseUrl = getEnv(
    "BUCKYOS_GATEWAY_BASE_URL",
    `https://${zoneHost}`,
  )!;
  const taskMgrGatewayUrl = getEnv(
    "BUCKYOS_TASK_MANAGER_GATEWAY_URL",
    "http://127.0.0.1:3180/kapi/task-manager",
  )!;
  const accountInfo = await buckyos.getAccountInfo();
  if (!accountInfo?.session_token) {
    throw new Error("missing session token for task-manager gateway RPC");
  }
  const taskMgr = new buckyos.kRPCClient(
    taskMgrGatewayUrl,
    accountInfo.session_token,
  ) as RpcClient;
  const tag = `${Date.now()}-${crypto.randomUUID().slice(0, 8)}`;
  const runner = `kevent-dv-${tag}`;
  const taskType = "kevent_kmsg.task_ready.dv";
  const sessionId = `kevent-kmsg-task-mgr-${tag}`;
  const eventId = `/task_mgr/runner/${runner}/task_ready`;
  const createdTaskIds: number[] = [];
  let stream: StreamReader | null = null;

  try {
    stream = await openKeventStream(gatewayBaseUrl, eventId);
    const ack = await stream.readFrame(5000);
    assert.equal(ack.type, "ack", "kevent stream should ack");

    const signaled = await createTask(taskMgr, {
      name: `task-ready-signal-${tag}`,
      taskType,
      runner,
      userId: ownerUserId || userId,
      appId,
      sessionId,
      data: { case: "DV-07", path: "event-driven", tag },
    });
    createdTaskIds.push(signaled.id);

    const eventData = await waitForTaskReadyEvent(stream, eventId, signaled.id);
    assert.equal(eventData.event_kind, "task_ready");
    assert.equal(eventData.task_id, signaled.id);
    assert.equal(eventData.runner, runner);
    assert.equal(eventData.status, "Pending");
    assert.equal(eventData.source_method, "create_task");

    const fetched = await rpcCall<JsonObject>(taskMgr, "get_task", {
      id: signaled.id,
    });
    const fetchedTask = asObject(fetched.task ?? fetched, "fetched task");
    assert.equal(fetchedTask.id, signaled.id);
    assert.equal(fetchedTask.runner, runner);
    assert.equal(fetchedTask.status, "Pending");

    stream.close();
    stream = null;

    const fallback = await createTask(taskMgr, {
      name: `task-ready-poll-${tag}`,
      taskType,
      runner,
      userId: ownerUserId || userId,
      appId,
      sessionId,
      data: { case: "DV-07", path: "polling-fallback", tag },
    });
    createdTaskIds.push(fallback.id);

    const listed = await listRunnerTasks(
      taskMgr,
      runner,
      taskType,
      ownerUserId || userId,
      appId,
    );
    const ids = new Set(listed.map((task) => task.id));
    assert.ok(ids.has(signaled.id), "list_tasks should find signaled task");
    assert.ok(ids.has(fallback.id), "list_tasks should find fallback task");

    console.log(
      JSON.stringify({
        status: "passed",
        case: "DV-07",
        event_id: eventId,
        runner,
        task_ids: createdTaskIds,
      }),
    );
  } finally {
    if (stream) stream.close();
    await Promise.all(createdTaskIds.map((id) => deleteTask(taskMgr, id)));
  }
}

await run();
