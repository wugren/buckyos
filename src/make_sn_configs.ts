#!/usr/bin/env -S deno run --allow-all
// make_sn_configs.ts - standalone minimal SN (Super Node) deployment tool.
// Split out of make_config.py's "sn" group: generates the full web3-gateway
// config dir and pre-registers the dev users, with buckyos-websdk provision
// only (no buckycli binary, no python).
//
// Runtime: Deno >= 2.2 (node:sqlite). websdk import mapped in src/deno.json.
//
// Usage: deno run --allow-all src/make_sn_configs.ts
//          [--rootfs <dir>] [--ca <dir>] [--sn_ip <ip>]
//          [--sn_base_host <host>] [--env_root <dir>] [--no-preregister]
//
// Output layout (matches make_config.py sn group):
//   <rootfs>/sn_device_config.json    SN server device config
//   <rootfs>/sn_private_key.pem       device private key (rtcp stack)
//   <rootfs>/params.json              SN service parameters (incl. sn_ip)
//   <rootfs>/sn_db.sqlite3            SN database with pre-registered users
//   <rootfs>/fullchain.cert/.pem      TLS cert+key for sn.$base, *.web3.$base
//   <rootfs>/ca/                      dev CA cert+key (client trust install)
//   <rootfs>/sn_server/.buckycli/     sn admin owner config
// Still created manually by the operator: dns_zone, website.yaml.
//
// User pre-registration: alice.ood1 / bob.ood1 / charlie.ood1 dev zones are
// registered into sn_db. Their user envs are reused from <env_root> when
// present (so JWTs match rootfs built by make_config.ts) and generated there
// otherwise (DEV_TEST_KEYS are deterministic, so devices still verify).

import * as fs from 'node:fs'
import * as path from 'node:path'
import * as os from 'node:os'
import { parseArgs } from 'node:util'
import {
  assertProvisionRuntime,
  createSnConfigs,
  registerUserToSn,
  registerDeviceToSn,
  ensureCa,
  createCertFromCa,
} from 'buckyos/provision'
import {
  ENV_ROOT_DIR,
  buildUserEnv,
  ensureDir,
  getParamsFromGroupName,
} from './make_config.ts'

const DEFAULT_SN_BASE_HOST = 'devtests.org'
const DEFAULT_CA_NAME = 'buckyos_test_ca'
const PREREGISTER_GROUPS = ['alice.ood1', 'bob.ood1', 'charlie.ood1']

function defaultTargetRoot(): string {
  if (os.platform() === 'win32') {
    const appData = Deno.env.get('APPDATA') ?? Deno.env.get('USERPROFILE') ?? '.'
    return path.join(appData, 'web3-gateway')
  }
  return '/opt/web3-gateway'
}

function getLocalIp(): string {
  for (const list of Object.values(os.networkInterfaces())) {
    for (const item of list ?? []) {
      if (!item.internal && item.family === 'IPv4') {
        return item.address
      }
    }
  }
  return '127.0.0.1'
}

async function makeSnConfigs(
  targetDir: string,
  snBaseHost: string,
  snIp: string,
  caDir: string,
  caName: string,
): Promise<void> {
  console.log(`Generating SN configuration files to ${targetDir} ...`)
  console.log(`  SN base domain: ${snBaseHost}`)
  console.log(`  SN IP address: ${snIp}`)
  ensureDir(targetDir)

  // 1. SN device identity configuration (written under <targetDir>/sn_server)
  console.log('# Step 1: Create SN device identity configuration...')
  await createSnConfigs({ outputDir: targetDir, snIp, snBaseHost })

  // move generated files up to targetDir; .buckycli stays in sn_server
  const snServerDir = path.join(targetDir, 'sn_server')
  if (fs.existsSync(snServerDir)) {
    for (const name of fs.readdirSync(snServerDir)) {
      const src = path.join(snServerDir, name)
      if (fs.statSync(src).isFile()) {
        fs.renameSync(src, path.join(targetDir, name))
        console.log(`Move file: ${name} -> ${targetDir}/`)
      }
    }
    if (fs.readdirSync(snServerDir).length === 0) {
      fs.rmdirSync(snServerDir)
    }
  }

  // 2. TLS certificates for sn.$base and *.web3.$base
  console.log('# Step 2: Generate TLS certificates...')
  await ensureCa(caDir, caName)
  const snHostname = `sn.${snBaseHost}`
  const { certPath, keyPath } = await createCertFromCa(caDir, snHostname, targetDir, [
    snHostname,
    `*.web3.${snBaseHost}`,
  ])
  fs.renameSync(certPath, path.join(targetDir, 'fullchain.cert'))
  fs.renameSync(keyPath, path.join(targetDir, 'fullchain.pem'))

  // keep dev CA cert+key next to the SN config for client trust install
  const caOutputDir = ensureDir(path.join(targetDir, 'ca'))
  for (const name of [`${caName}_ca_cert.pem`, `${caName}_ca_key.pem`]) {
    fs.copyFileSync(path.join(caDir, name), path.join(caOutputDir, name))
  }
  console.log('TLS certificates generated:')
  console.log(`  - ${path.join(targetDir, 'fullchain.cert')}`)
  console.log(`  - ${path.join(targetDir, 'fullchain.pem')}`)
  console.log(`  - ${path.join(caOutputDir, `${caName}_ca_cert.pem`)}`)
}

async function preregisterDevUsers(envRoot: string, snDbPath: string): Promise<void> {
  console.log('# Step 3: Pre-register dev users to SN database...')
  for (const groupName of PREREGISTER_GROUPS) {
    const params = getParamsFromGroupName(groupName)
    const userDir = path.join(envRoot, params.zone_id)
    if (!fs.existsSync(path.join(userDir, params.node_name, 'node_identity.json'))) {
      console.log(`user env missing, generate: ${userDir}`)
      await buildUserEnv(params, envRoot)
    } else {
      console.log(`reuse existing user env: ${userDir}`)
    }
    await registerUserToSn(envRoot, params.zone_id, snDbPath)
    await registerDeviceToSn(envRoot, params.zone_id, params.node_name, snDbPath)
  }
}

async function main(): Promise<void> {
  try {
    assertProvisionRuntime()
  } catch (e) {
    console.error(`runtime check failed: ${e instanceof Error ? e.message : e}`)
    console.error('make_sn_configs.ts requires Deno >= 2.2 (node:sqlite)')
    Deno.exit(1)
  }

  const { values, positionals } = parseArgs({
    args: Deno.args,
    options: {
      rootfs: { type: 'string' },
      ca: { type: 'string' },
      sn_ip: { type: 'string' },
      sn_base_host: { type: 'string' },
      env_root: { type: 'string' },
      'no-preregister': { type: 'boolean' },
    },
    allowPositionals: true,
  })
  if (positionals.length > 0) {
    console.error('usage: make_sn_configs.ts [--rootfs <dir>] [--ca <dir>] [--sn_ip <ip>] [--sn_base_host <host>] [--env_root <dir>] [--no-preregister]')
    Deno.exit(1)
  }

  const targetDir = values.rootfs ?? defaultTargetRoot()
  const snBaseHost = values.sn_base_host ?? DEFAULT_SN_BASE_HOST
  const snIp = values.sn_ip ?? Deno.env.get('BUCKYOS_SN_IP') ?? getLocalIp()
  const envRoot = values.env_root ?? ENV_ROOT_DIR
  const caDir = values.ca ?? ensureDir(path.join(ENV_ROOT_DIR, 'ca'))

  await makeSnConfigs(targetDir, snBaseHost, snIp, caDir, DEFAULT_CA_NAME)

  if (!values['no-preregister']) {
    await preregisterDevUsers(envRoot, path.join(targetDir, 'sn_db.sqlite3'))
  }

  console.log('\n[OK] SN configuration files generation completed!')
  console.log(`  Output directory: ${targetDir}`)
  console.log('Files that need to be manually created:')
  console.log(`  - ${path.join(targetDir, 'dns_zone')} (DNS Zone configuration)`)
  console.log(`  - ${path.join(targetDir, 'website.yaml')} (website configuration)`)
  console.log('Other notes:')
  console.log('  - Test environment needs to install the CA certificate into the client trust list')
}

if (import.meta.main) {
  await main()
}
