# Phase 20: npm Distribution & Authentication

**Priority:** LOW (Post-MVP)
**Effort:** 15-20 hours
**Dependencies:** Phase 14 (Enhanced CLI)

---

## Overview

Make StreamHouse CLI installable via npm with simple authentication and shorter command syntax, similar to popular tools like Vercel, Railway, or Supabase CLIs.

---

## Goals

### 1. npm Installation

```bash
# Install globally
npm install -g streamhouse

# Or use with npx
npx streamhouse login
```

### 2. Authentication & Session Management

```bash
# Login to instance (saves credentials)
streamhouse login https://my-streamhouse.com
# Or shorter alias:
sh login https://my-streamhouse.com

# Credentials stored in ~/.streamhouse/auth.json
# Automatic token refresh
```

### 3. Simplified Commands

```bash
# After login, commands use saved instance
sh list topics
sh create topic orders --partitions 12
sh produce orders '{"amount": 99.99}'
sh consume orders
sh schema list
sh schema register orders-value schema.avsc
```

---

## Implementation Strategy

### 1. npm Package Structure

Similar to how `esbuild`, `swc`, or `prisma` distribute Rust binaries via npm:

```
streamhouse-cli/
├── package.json          # npm package metadata
├── install.js            # Post-install script
├── bin/
│   ├── streamhouse       # Wrapper script
│   └── sh                # Short alias
└── native/
    ├── streamhouse-darwin-arm64
    ├── streamhouse-darwin-x64
    ├── streamhouse-linux-x64
    ├── streamhouse-linux-arm64
    └── streamhouse-win32-x64.exe
```

**package.json:**
```json
{
  "name": "streamhouse",
  "version": "0.1.0",
  "description": "StreamHouse CLI - Real-time streaming platform",
  "bin": {
    "streamhouse": "./bin/streamhouse",
    "sh": "./bin/sh"
  },
  "scripts": {
    "postinstall": "node install.js"
  },
  "optionalDependencies": {
    "@streamhouse/darwin-arm64": "0.1.0",
    "@streamhouse/darwin-x64": "0.1.0",
    "@streamhouse/linux-x64": "0.1.0",
    "@streamhouse/linux-arm64": "0.1.0",
    "@streamhouse/win32-x64": "0.1.0"
  }
}
```

**install.js:**
```javascript
const { platform, arch } = process;
const path = require('path');
const fs = require('fs');

// Download/copy appropriate binary for platform
const binaryName = `streamhouse-${platform}-${arch}${platform === 'win32' ? '.exe' : ''}`;
// ... installation logic
```

### 2. Authentication System

**Login Flow:**
```
$ streamhouse login https://my-instance.com
Opening browser for authentication...
✓ Authenticated as user@example.com
✓ Credentials saved to ~/.streamhouse/auth.json
```

**Auth Storage (`~/.streamhouse/auth.json`):**
```json
{
  "default_instance": "https://my-instance.com",
  "instances": {
    "https://my-instance.com": {
      "access_token": "eyJ...",
      "refresh_token": "eyJ...",
      "expires_at": 1738512000,
      "user": {
        "email": "user@example.com",
        "name": "John Doe"
      }
    }
  }
}
```

**Implementation:**
- OAuth2 device flow (no browser required) OR
- Local server with callback (opens browser)
- JWT token storage with automatic refresh
- Multiple instance support (switch between dev/staging/prod)

### 3. Command Aliases

**Current:**
```bash
streamctl --server http://localhost:9090 topic list
streamctl --schema-registry-url http://localhost:8081 schema list
```

**New (after login):**
```bash
sh list topics
sh schema list
```

**Alias Mapping:**
```
sh = streamhouse
sh list topics = streamhouse topic list
sh create topic = streamhouse topic create
sh produce = streamhouse produce
sh consume = streamhouse consume
sh schema = streamhouse schema
```

---

## Technical Implementation

### 1. Create npm Package (~5h)

**Files to create:**
- `npm/package.json` - npm package metadata
- `npm/install.js` - Binary download/installation
- `npm/bin/streamhouse` - Wrapper script
- `npm/bin/sh` - Short alias
- `npm/README.md` - npm-specific docs

**Build process:**
```bash
# Cross-compile for all platforms
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
cargo build --release --target x86_64-pc-windows-msvc

# Package for npm
npm/build-npm-packages.sh
```

**Publishing:**
```bash
cd npm
npm publish
```

### 2. Authentication Module (~8h)

**New file: `crates/streamhouse-cli/src/auth.rs`**

```rust
pub struct AuthManager {
    config_path: PathBuf,
}

impl AuthManager {
    pub async fn login(&self, instance_url: &str) -> Result<()> {
        // OAuth device flow or local server
        let tokens = self.authenticate(instance_url).await?;
        self.save_credentials(instance_url, tokens)?;
        Ok(())
    }

    pub async fn get_token(&self, instance_url: &str) -> Result<String> {
        let creds = self.load_credentials(instance_url)?;

        if self.is_expired(&creds) {
            self.refresh_token(instance_url).await
        } else {
            Ok(creds.access_token)
        }
    }

    pub fn logout(&self, instance_url: &str) -> Result<()> {
        // Remove credentials
    }

    pub fn list_instances(&self) -> Result<Vec<String>> {
        // List all logged-in instances
    }

    pub fn set_default_instance(&self, instance_url: &str) -> Result<()> {
        // Set default instance
    }
}
```

**New commands:**
```rust
Commands::Login { instance_url } => handle_login(instance_url).await?,
Commands::Logout { instance_url } => handle_logout(instance_url).await?,
Commands::Whoami => handle_whoami().await?,
```

### 3. Command Aliases (~2h)

**Modify `main.rs` to support aliases:**

```rust
fn main() -> Result<()> {
    // Check if invoked as 'sh'
    let binary_name = std::env::args().next().unwrap();
    if binary_name.ends_with("/sh") || binary_name.ends_with("\\sh.exe") {
        // Parse with alias syntax
        // "sh list topics" -> "streamhouse topic list"
        parse_alias_command()
    } else {
        // Normal parsing
        Cli::parse()
    }
}

fn parse_alias_command() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Map aliases
    let mapped_args = match args.as_slice() {
        ["list", "topics"] => vec!["topic", "list"],
        ["create", "topic", rest @ ..] => {
            let mut v = vec!["topic", "create"];
            v.extend_from_slice(rest);
            v
        }
        _ => args,
    };

    // Parse as normal
    // ...
}
```

### 4. Automatic Instance Connection (~3h)

**Load instance from auth config:**

```rust
async fn main() -> Result<()> {
    // Load auth config
    let auth = AuthManager::load()?;

    // Get default instance
    let instance_url = cli.server.or_else(|| auth.default_instance());

    // Get auth token
    let token = auth.get_token(&instance_url).await?;

    // Create authenticated client
    let channel = create_authenticated_channel(&instance_url, &token).await?;

    // ...
}
```

---

## User Experience

### First Time Setup

```bash
# Install
$ npm install -g streamhouse
✓ Installed streamhouse CLI v0.1.0

# Login
$ streamhouse login https://my-streamhouse.com
Opening browser for authentication...
✓ Authenticated as john@example.com
✓ Default instance set to https://my-streamhouse.com

# Start using
$ sh list topics
Topics (0):
  (no topics yet)

$ sh create topic orders --partitions 12
✓ Topic created: orders (12 partitions)

$ sh schema list
Subjects (0):
  (no subjects yet)
```

### Multi-Instance Management

```bash
# Add staging instance
$ streamhouse login https://staging.streamhouse.com

# List instances
$ streamhouse instances
Instances:
  https://my-streamhouse.com (default) - john@example.com
  https://staging.streamhouse.com - john@example.com

# Use specific instance
$ sh --instance staging list topics
Topics (3):
  orders (12 partitions)
  events (6 partitions)
  logs (3 partitions)

# Switch default
$ streamhouse use https://staging.streamhouse.com
✓ Default instance set to https://staging.streamhouse.com
```

---

## Security Considerations

### 1. Token Storage

- Store in `~/.streamhouse/auth.json` with 0600 permissions
- Encrypt tokens using platform keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- Never log tokens

### 2. Token Refresh

- Automatic refresh before expiration
- Graceful re-authentication if refresh fails
- Clear error messages for auth failures

### 3. HTTPS Enforcement

- Require HTTPS for production instances
- Allow HTTP only for localhost
- Certificate validation

---

## Alternative: Single Binary Distribution

If npm distribution is complex, alternative approach:

```bash
# Install script (similar to Deno, Bun)
curl -fsSL https://streamhouse.io/install.sh | sh

# Or with Homebrew
brew install streamhouse/tap/streamhouse

# Or download from GitHub releases
https://github.com/streamhouse/streamhouse/releases
```

---

## Comparison to Similar Tools

### Vercel CLI
```bash
npm i -g vercel
vercel login
vercel list
```

### Railway CLI
```bash
npm i -g @railway/cli
railway login
railway list
```

### Supabase CLI
```bash
npm i -g supabase
supabase login
supabase projects list
```

### Our Approach
```bash
npm i -g streamhouse
streamhouse login
sh list topics
```

---

## Roadmap

### Phase 20.1: npm Package (~5h)
- Create package.json and build scripts
- Cross-compile for all platforms
- Test installation on macOS/Linux/Windows
- Publish to npm

### Phase 20.2: Authentication (~8h)
- Implement OAuth device flow
- Token storage and refresh
- Login/logout commands
- Multi-instance support

### Phase 20.3: Command Aliases (~2h)
- Add `sh` binary
- Implement alias parsing
- Update help text

### Phase 20.4: Documentation (~2h)
- Installation guide
- Authentication guide
- Migration guide from manual install
- Update README

### Phase 20.5: Testing (~3h)
- Installation tests
- Auth flow tests
- Alias tests
- E2E user journey tests

**Total: 20 hours**

---

## Success Criteria

✅ `npm install -g streamhouse` installs CLI
✅ `streamhouse login` authenticates user
✅ `sh list topics` works after login
✅ Tokens refresh automatically
✅ Multi-instance support works
✅ Works on macOS, Linux, Windows
✅ Documentation complete

---

## Future Enhancements

1. **Plugin System**
   - `sh plugin install streamhouse-ui` - Install web UI
   - `sh plugin install streamhouse-k8s` - Kubernetes integration

2. **Interactive Mode**
   - `sh` with no args enters interactive mode
   - Tab completion for resources

3. **Configuration Profiles**
   - `sh --profile production list topics`
   - Multiple profiles per instance

4. **Cloud-hosted Instances**
   - `streamhouse deploy` - Deploy to managed hosting
   - `streamhouse scale` - Auto-scaling configuration

---

**Phase 20: Deferred to Post-MVP**
**Dependency:** Needs authentication/authorization system in server
**Estimated:** 20 hours
