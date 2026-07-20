// Loads the platform-specific native addon: a local dev build first
// (`npm run build` copies it here as `rs-minimatch.node`), falling back to
// whichever `rs-minimatch-<platform>-<arch>` optionalDependency npm
// actually installed for this machine (see package.json and
// .github/workflows/release.yml, which builds and publishes one per
// target).
'use strict'

const { existsSync } = require('fs')
const path = require('path')

const localBinding = path.join(__dirname, 'rs-minimatch.node')

let binding
if (existsSync(localBinding)) {
  binding = require(localBinding)
} else {
  try {
    binding = require(`rs-minimatch-${process.platform}-${process.arch}`)
  } catch {
    throw new Error(
      `rs-minimatch: no native binding found for ${process.platform}-${process.arch}. ` +
        'Run `npm run build` in this package (or `cargo build -p rs-minimatch-napi --release` ' +
        'and copy the resulting library to rs-minimatch.node) or install a matching prebuilt package.'
    )
  }
}

module.exports = binding
