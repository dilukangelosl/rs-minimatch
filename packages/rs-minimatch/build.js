// Builds the Rust NAPI addon and copies it here as `rs-minimatch.node`, the
// name native.js looks for. A real release pipeline publishes one
// `rs-minimatch-<platform>-<arch>` package per target instead (see
// .github/workflows/release.yml).
'use strict'

const { execFileSync } = require('child_process')
const { copyFileSync, existsSync } = require('fs')
const path = require('path')

const repoRoot = path.join(__dirname, '..', '..')
const profile = process.env.RS_MINIMATCH_PROFILE === 'debug' ? 'debug' : 'release'

execFileSync('cargo', ['build', '-p', 'rs-minimatch-napi', ...(profile === 'release' ? ['--release'] : [])], {
  cwd: repoRoot,
  stdio: 'inherit',
})

const libNames = {
  darwin: 'librs_minimatch_napi.dylib',
  linux: 'librs_minimatch_napi.so',
  win32: 'rs_minimatch_napi.dll',
}
const libName = libNames[process.platform]
if (!libName) {
  throw new Error(`rs-minimatch: unsupported platform ${process.platform}`)
}

const built = path.join(repoRoot, 'target', profile, libName)
if (!existsSync(built)) {
  throw new Error(`rs-minimatch: expected build output at ${built}`)
}

copyFileSync(built, path.join(__dirname, 'rs-minimatch.node'))
console.log(`rs-minimatch: wrote ${path.join(__dirname, 'rs-minimatch.node')}`)
