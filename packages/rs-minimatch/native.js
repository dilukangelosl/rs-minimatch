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

// npm's package registry name for the win32 package is "windows", not
// "win32" - npm's automated spam filter blocks new, unscoped package names
// combining "win32" with a number (like "win32-x64") outright. This is
// purely a package-naming workaround; process.platform itself is still
// (and always will be) "win32" on Windows.
const platformName = process.platform === 'win32' ? 'windows' : process.platform

let binding
if (existsSync(localBinding)) {
  binding = require(localBinding)
} else {
  try {
    binding = require(`rs-minimatch-${platformName}-${process.arch}`)
  } catch {
    throw new Error(
      `rs-minimatch: no native binding found for ${process.platform}-${process.arch}. ` +
        'Run `npm run build` in this package (or `cargo build -p rs-minimatch-napi --release` ' +
        'and copy the resulting library to rs-minimatch.node) or install a matching prebuilt package.'
    )
  }
}

module.exports = binding
