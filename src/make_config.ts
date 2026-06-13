#!/usr/bin/env -S deno run --allow-all
// make_config.ts - TypeScript rewrite of make_config.py.
// Builds all config files of a target rootfs with buckyos-websdk provision:
// no buckycli binary, no python/buckyos_devkit required.
//
// Runtime: Deno >= 2.2 (node:sqlite). The websdk import is mapped in
// src/deno.json (buckyos/provision -> buckyos-websdk dist). For local websdk
// development point that import at your local dist/provision.mjs.
//
// Usage: deno run --allow-all src/make_config.ts <group> [--rootfs <dir>] [--ca <dir>]
//   groups: dev | devtest_ood1 | alice.ood1 | bob.ood1 | charlie.ood1 |
//           devtests_ood1 | sn_web | release
//
// Removed relative to make_config.py (by design, 2026-06 review):
// - meta_index.db.fileobj fake cache (make_repo_cache_file / global env part)
// - did_docs cache preheat (make_cache_did_docs)
// - bin pkg meta seeding (seed_bin_pkg_meta_db)
// SN config generation moved to the standalone src/make_sn_configs.ts.

import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { parseArgs } from "node:util";
import {
  assertProvisionRuntime,
  createNodeConfigs,
  createUserEnv,
  ensureCa,
  createCertFromCa,
} from "buckyos/provision";

// ============================================================================
// shared paths & helpers (mirror buckyos_devkit get_buckyos_root / BUCKYCLI_DIR)
// ============================================================================

const IS_WINDOWS = os.platform() === "win32";

// Historical location of the buckycli working dir; user envs and the dev CA
// keep living here so TS- and buckycli-generated environments interoperate.
export const ENV_ROOT_DIR = path.join(os.homedir(), "buckycli");

export function getBuckyosRoot(): string {
  const fromEnv = Deno.env.get("BUCKYOS_ROOT");
  if (fromEnv) {
    return fromEnv;
  }
  if (IS_WINDOWS) {
    const appData = Deno.env.get("APPDATA") ?? Deno.env.get("USERPROFILE") ??
      ".";
    return path.join(appData, "buckyos");
  }
  return "/opt/buckyos/";
}

export function ensureDir(dirPath: string): string {
  fs.mkdirSync(dirPath, { recursive: true });
  return dirPath;
}

function copyIfExists(src: string, dst: string): void {
  if (!fs.existsSync(src)) {
    console.log(`skip missing file: ${src}`);
    return;
  }
  ensureDir(path.dirname(dst));
  fs.copyFileSync(src, dst);
  console.log(`copy ${src} -> ${dst}`);
}

function writeJson(filePath: string, data: unknown): void {
  ensureDir(path.dirname(filePath));
  fs.writeFileSync(filePath, JSON.stringify(data, null, 2) + "\n");
  console.log(`write json ${filePath}`);
}

function writeText(filePath: string, content: string): void {
  ensureDir(path.dirname(filePath));
  fs.writeFileSync(filePath, content);
  console.log(`write content ${filePath}`);
}

// ============================================================================
// group parameter book
// ============================================================================

export interface OODGroupParams {
  username: string;
  zone_id: string;
  node_name: string;
  netid: string;
  rtcp_port: number;
  sn_base_host: string;
  web3_bridge: string;
  trust_did: string[];
  force_https: boolean;
  ca_name: string;
}

const DEFAULT_TRUST_DID = [
  "did:web:buckyos.org",
  "did:web:buckyos.ai",
  "did:web:buckyos.io",
];

const OOD_GROUPS: Record<string, OODGroupParams> = {
  dev: {
    username: "devtest",
    zone_id: "test.buckyos.io",
    node_name: "ood1",
    netid: "wan",
    rtcp_port: 2980,
    sn_base_host: "",
    web3_bridge: "web3.devtests.org",
    trust_did: DEFAULT_TRUST_DID,
    force_https: false,
    ca_name: "buckyos_test_ca",
  },
  "alice.ood1": {
    username: "alice",
    zone_id: "alice.bns.did",
    node_name: "ood1",
    netid: "lan",
    rtcp_port: 2980,
    sn_base_host: "devtests.org",
    web3_bridge: "web3.devtests.org",
    trust_did: DEFAULT_TRUST_DID,
    force_https: false,
    ca_name: "buckyos_test_ca",
  },
  "bob.ood1": {
    username: "bob",
    zone_id: "bob.bns.did",
    node_name: "ood1",
    // netid is wan but has SN, means need to use d-dns
    netid: "wan_dyn",
    rtcp_port: 2980,
    sn_base_host: "devtests.org",
    web3_bridge: "web3.devtests.org",
    trust_did: DEFAULT_TRUST_DID,
    force_https: false,
    ca_name: "buckyos_test_ca",
  },
  "charlie.ood1": {
    username: "charlie",
    zone_id: "charlie.me",
    // portmap: https goes through relay, rtcp can connect directly
    node_name: "ood1",
    netid: "portmap",
    rtcp_port: 2981,
    sn_base_host: "devtests.org",
    web3_bridge: "web3.devtests.org",
    trust_did: DEFAULT_TRUST_DID,
    force_https: false,
    ca_name: "buckyos_test_ca",
  },
  devtests_ood1: {
    username: "devtests",
    zone_id: "devtests.org",
    node_name: "ood1",
    netid: "wan",
    rtcp_port: 2980,
    sn_base_host: "",
    web3_bridge: "web3.devtests.org",
    trust_did: DEFAULT_TRUST_DID,
    force_https: false,
    ca_name: "buckyos_test_ca",
  },
};
OOD_GROUPS["devtest_ood1"] = OOD_GROUPS["dev"];
OOD_GROUPS["sn_web"] = OOD_GROUPS["devtests_ood1"];

export function getParamsFromGroupName(groupName: string): OODGroupParams {
  const params = OOD_GROUPS[groupName];
  if (!params) {
    throw new Error(`invalid group name: ${groupName}`);
  }
  return params;
}

// ============================================================================
// global env config (machine.json + active_config.json)
// ============================================================================

// Extract the base domain from web3_bns, e.g. web3.devtests.org -> devtests.org
function extractBaseHost(web3Bns: string): string {
  if (web3Bns.startsWith("web3.")) {
    return web3Bns.slice(5);
  }
  const dot = web3Bns.indexOf(".");
  if (dot >= 0) {
    return web3Bns.slice(dot + 1);
  }
  return web3Bns;
}

export function didHostToRealHost(didHost: string, web3Bridge: string): string {
  if (didHost.endsWith(".bns.did")) {
    const result = didHost.split(".bns.did")[0] + "." + web3Bridge;
    console.log(`did_host_to_real_host: ${didHost} -> ${result}`);
    return result;
  }
  return didHost;
}

function makeGlobalEnvConfig(
  targetDir: string,
  web3Bns: string,
  trustDid: string[],
  forceHttps: boolean,
): void {
  const etcDir = ensureDir(path.join(targetDir, "etc"));

  writeJson(path.join(etcDir, "machine.json"), {
    web3_bridge: { bns: web3Bns },
    force_https: forceHttps,
    trust_did: trustDid,
  });

  const activeConfigPath = path.join(
    targetDir,
    "bin",
    "node-active",
    "active_config.json",
  );
  writeJson(activeConfigPath, {
    sn_base_host: extractBaseHost(web3Bns),
    http_schema: forceHttps ? "https" : "http",
  });
  console.log(`create active config at ${activeConfigPath}`);
}

// ============================================================================
// user env construction (shared with make_sn_configs.ts)
// ============================================================================

// Build (or rebuild) the user env dir <envRoot>/<zone_id> with provision.
// Mirrors the create_user_env + create_node_configs part of make_identity_files.
export async function buildUserEnv(
  params: OODGroupParams,
  envRoot: string,
): Promise<string> {
  const userDir = ensureDir(path.join(envRoot, params.zone_id));
  const oodNameForZone = params.netid !== "lan"
    ? `${params.node_name}@${params.netid}`
    : params.node_name;

  await createUserEnv({
    username: params.username,
    hostname: params.zone_id,
    oodName: oodNameForZone,
    snBaseHost: params.sn_base_host,
    rtcpPort: params.rtcp_port,
    outputDir: userDir,
  });
  await createNodeConfigs({
    deviceName: params.node_name,
    netId: params.netid,
    envDir: userDir,
  });
  return userDir;
}

// ============================================================================
// identity files + TLS
// ============================================================================

function copyIdentityOutputs(
  userDir: string,
  nodeDir: string,
  targetDir: string,
  zoneId: string,
): void {
  const etcDir = ensureDir(path.join(targetDir, "etc"));

  copyIfExists(
    path.join(userDir, `${zoneId}.zone.json`),
    path.join(etcDir, `${zoneId}.zone.json`),
  );
  for (
    const name of [
      "start_config.json",
      "node_identity.json",
      "node_private_key.pem",
      "node_device_config.json",
    ]
  ) {
    copyIfExists(path.join(nodeDir, name), path.join(etcDir, name));
  }

  const buckycliDir = ensureDir(path.join(etcDir, ".buckycli"));
  for (const name of ["user_config.json", "user_private_key.pem"]) {
    copyIfExists(path.join(userDir, name), path.join(buckycliDir, name));
  }
  copyIfExists(
    path.join(userDir, `${zoneId}.zone.json`),
    path.join(buckycliDir, "zone_config.json"),
  );
}

async function generateTls(
  zoneId: string,
  caName: string,
  etcDir: string,
  caDir: string,
): Promise<void> {
  await ensureCa(caDir, caName);
  const { certPath, keyPath } = await createCertFromCa(caDir, zoneId, etcDir, [
    zoneId,
    `*.${zoneId}`,
  ]);

  fs.renameSync(certPath, path.join(etcDir, "zone_cert.cert"));
  fs.renameSync(keyPath, path.join(etcDir, "zone_cert_key.pem"));

  // Keep CA for trust
  copyIfExists(
    path.join(caDir, `${caName}_ca_cert.pem`),
    path.join(etcDir, "ca.cert"),
  );
  copyIfExists(
    path.join(caDir, `${caName}_ca_key.pem`),
    path.join(etcDir, "ca_key.pem"),
  );
  console.log(`tls certs generated under ${etcDir}`);

  const postGatewayConfig = `
stacks:
  zone_gateway_https:
    bind: 0.0.0.0:443
    protocol: tls
    certs:
      - domain: "${zoneId}"
        cert_path: ./zone_cert.cert
        key_path: ./zone_cert_key.pem
      - domain: "*.${zoneId}"
        cert_path: ./zone_cert.cert
        key_path: ./zone_cert_key.pem
    hook_point:
      main:
        id: main
        priority: 1
        blocks:
          default:
            id: default
            priority: 1
            block: |
              return "server node_gateway";
    `;
  writeText(path.join(etcDir, "post_gateway.yaml"), postGatewayConfig);
}

async function makeIdentityFiles(
  targetDir: string,
  params: OODGroupParams,
  caDir: string,
): Promise<void> {
  const userDir = await buildUserEnv(params, ENV_ROOT_DIR);
  const nodeDir = path.join(userDir, params.node_name);
  copyIdentityOutputs(userDir, nodeDir, targetDir, params.zone_id);

  await generateTls(
    didHostToRealHost(params.zone_id, params.web3_bridge),
    params.ca_name,
    ensureDir(path.join(targetDir, "etc")),
    caDir,
  );
}

// ============================================================================
// dev boot template override (~/.buckycli/buckyos_boot.toml merge)
// ============================================================================

// Matches TOML entries of the form: "key" = """\n<value>\n"""
const BOOT_ENTRY_PATTERN =
  /^[^\S\n]*"([^"\n]+)"[^\S\n]*=[^\S\n]*"""\n([\s\S]*?)\n"""[ \t]*\n?/gm;

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function applyDevBootTemplateOverride(
  targetDir: string,
  groupName: string,
): void {
  if (groupName !== "dev" && groupName !== "devtest_ood1") {
    return;
  }

  const localBootToml = path.join(
    os.homedir(),
    ".buckycli",
    "buckyos_boot.toml",
  );
  if (!fs.existsSync(localBootToml)) {
    console.log(`skip missing dev boot override: ${localBootToml}`);
    return;
  }

  const dstBootTemplate = path.join(
    targetDir,
    "etc",
    "scheduler",
    "boot.template.toml",
  );
  const srcText = fs.readFileSync(localBootToml, "utf8");
  const entries: Array<[string, string]> = [];
  for (const match of srcText.matchAll(BOOT_ENTRY_PATTERN)) {
    const key = match[1];
    console.log(`load ${key} from .buckycli/boot_template`);
    entries.push([key, `"${key}" = """\n${match[2]}\n"""`]);
  }

  if (entries.length === 0) {
    console.log(
      `skip invalid dev boot override (no key/value found): ${localBootToml}`,
    );
    return;
  }

  if (!fs.existsSync(dstBootTemplate)) {
    writeText(dstBootTemplate, srcText);
    console.log(`create boot template from local override: ${dstBootTemplate}`);
    return;
  }

  let merged = fs.readFileSync(dstBootTemplate, "utf8");
  let replaced = 0;
  let added = 0;
  for (const [key, block] of entries) {
    const targetPattern = new RegExp(
      `^[^\\S\\n]*"${
        escapeRegExp(key)
      }"[^\\S\\n]*=[^\\S\\n]*"""\\n[\\s\\S]*?\\n"""[ \\t]*\\n?`,
      "m",
    );
    if (targetPattern.test(merged)) {
      merged = merged.replace(targetPattern, block + "\n");
      replaced += 1;
      continue;
    }

    if (merged && !merged.endsWith("\n")) {
      merged += "\n";
    }
    if (merged && !merged.endsWith("\n\n")) {
      merged += "\n";
    }
    merged += block + "\n";
    added += 1;
  }

  writeText(dstBootTemplate, merged);
  console.log(
    `merge dev boot override into template: ${dstBootTemplate} (replaced=${replaced}, added=${added})`,
  );
}

// ============================================================================
// main
// ============================================================================

function printUsage(log: (message?: unknown) => void = console.error): void {
  log("usage: make_config.ts <group> [--rootfs <dir>] [--ca <dir>]");
  log(`groups: ${[...Object.keys(OOD_GROUPS), "release"].join(" | ")}`);
  log(
    "sn configs: deno run make_sn_configs [--rootfs <dir>] [--ca <dir>] [--sn_ip <ip>]",
  );
}

export async function makeConfigByGroupName(
  groupName: string,
  targetRoot: string | undefined,
  caDir: string | undefined,
): Promise<void> {
  if (groupName === "sn" || groupName === "sn_server") {
    throw new Error(
      "SN config generation moved to the standalone tool: deno run --allow-all src/make_sn_configs.ts",
    );
  }

  if (groupName === "release") {
    const targetDir = targetRoot ?? getBuckyosRoot();
    console.log(`release mode, write basic configs to ${targetDir}`);
    makeGlobalEnvConfig(targetDir, "web3.buckyos.ai", DEFAULT_TRUST_DID, true);
    return;
  }

  const params = getParamsFromGroupName(groupName);
  const targetDir = targetRoot ?? getBuckyosRoot();
  const resolvedCaDir = caDir ?? ensureDir(path.join(ENV_ROOT_DIR, "ca"));

  console.log(
    `############ make config for group name: ${groupName} #########################`,
  );
  console.log(`rootfs dir : ${targetDir}`);
  console.log(`group      : ${groupName}`);
  console.log(`username   : ${params.username}`);
  console.log(`zone       : ${params.zone_id}`);
  console.log(`node       : ${params.node_name}`);
  console.log(`web3_bridge: ${params.web3_bridge}`);

  makeGlobalEnvConfig(
    targetDir,
    params.web3_bridge,
    params.trust_did,
    params.force_https,
  );
  await makeIdentityFiles(targetDir, params, resolvedCaDir);
  applyDevBootTemplateOverride(targetDir, groupName);

  console.log(`config ${groupName} generation finished.`);
}

async function main(): Promise<void> {
  let values: { rootfs?: string; ca?: string; sn_ip?: string; help?: boolean };
  let positionals: string[];
  try {
    const parsed = parseArgs({
      args: Deno.args,
      options: {
        rootfs: { type: "string" },
        ca: { type: "string" },
        sn_ip: { type: "string" },
        help: { type: "boolean", short: "h" },
      },
      allowPositionals: true,
    }) as {
      values: { rootfs?: string; ca?: string; sn_ip?: string; help?: boolean };
      positionals: string[];
    };
    values = parsed.values;
    positionals = parsed.positionals;
  } catch (e) {
    console.error(`argument error: ${e instanceof Error ? e.message : e}`);
    printUsage();
    Deno.exit(1);
  }

  if (values.help) {
    printUsage(console.log);
    return;
  }

  if (positionals.length !== 1) {
    console.error(
      `argument error: expected exactly one group, got ${positionals.length}`,
    );
    printUsage();
    Deno.exit(1);
  }

  const groupName = positionals[0];
  if (values.sn_ip && groupName !== "sn" && groupName !== "sn_server") {
    console.error(
      "argument error: --sn_ip is only supported by src/make_sn_configs.ts",
    );
    printUsage();
    Deno.exit(1);
  }

  try {
    assertProvisionRuntime();
  } catch (e) {
    console.error(
      `runtime check failed: ${e instanceof Error ? e.message : e}`,
    );
    console.error(
      "make_config.ts requires Deno >= 2.2 (or Node >= 22.13 with a loader for the websdk import)",
    );
    Deno.exit(1);
  }

  try {
    await makeConfigByGroupName(groupName, values.rootfs, values.ca);
  } catch (e) {
    console.error(
      `config generation failed: ${e instanceof Error ? e.message : e}`,
    );
    printUsage();
    Deno.exit(1);
  }
}

if (import.meta.main) {
  await main();
}
