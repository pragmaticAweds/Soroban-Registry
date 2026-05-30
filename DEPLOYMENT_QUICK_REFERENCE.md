# Contract Deployment CLI - Quick Reference Guide

## Installation

The feature is built into the Soroban Registry CLI. Compile and install:

```bash
cd cli
cargo build --release
```

## Quick Start

### 1. Basic Deployment (Simplest)

```bash
soroban-registry contract deploy ./my_contract.wasm \
  --name "MyContract" \
  --network testnet
```

**What it does:**
- ✓ Validates WASM file
- ✓ Computes contract hash
- ✓ Registers in database
- ✓ Returns deployment ID

### 2. Full-Featured Deployment

```bash
soroban-registry contract deploy ./my_contract.wasm \
  --name "YieldOptimizer" \
  --description "Maximizes DeFi yields across protocols" \
  --category DeFi \
  --network mainnet \
  --icon ./logo.png \
  --tags "yield,defi,optimization" \
  --publisher GB7YVZBXVCD5SV3RL5FIWYXSVS7UBQKILO5R4RXPJNBG5LT5VZIXC7K
```

### 3. Interactive Mode (Guided Prompts)

```bash
soroban-registry contract deploy ./my_contract.wasm --interactive
```

Prompts for:
- Contract name
- Description (optional)
- Category (1-6 selection)
- Network (1-3 selection)
- Tags (optional)
- Icon path (optional)

### 4. JSON Output (For Scripts)

```bash
soroban-registry contract deploy ./my_contract.wasm \
  --name "MyContract" \
  --network testnet \
  --json
```

## Command Reference

```
USAGE:
    soroban-registry contract deploy <WASM_PATH> [OPTIONS]

ARGS:
    <WASM_PATH>    Path to the WASM binary file

OPTIONS:
    --name <NAME>                Contract name (required if not interactive)
    --description <DESC>         Contract description (optional)
    --category <CAT>             Category: DeFi, Token, Oracle, NFT, Utility, Other
    --network <NET>              Network: mainnet | testnet | futurenet (default: testnet)
    --icon <ICON_PATH>           Icon file path (PNG, JPG, SVG - max 2MB)
    --tags <TAGS>                Comma-separated tags
    --publisher <ADDRESS>        Publisher Stellar address
    --interactive                Enable interactive guided mode
    --skip-abi                   Skip ABI extraction from WASM
    --json                       Output results as JSON
    -h, --help                   Print help information
```

## Examples by Use Case

### Scenario 1: New Developer (First Deployment)

```bash
# Use interactive mode for guided setup
soroban-registry contract deploy ./contract.wasm --interactive
```

### Scenario 2: Established Team (Automated CI/CD)

```bash
# Use JSON output for scripting
soroban-registry contract deploy ./contract.wasm \
  --name "$CONTRACT_NAME" \
  --description "$CONTRACT_DESC" \
  --category "$CATEGORY" \
  --network "$NETWORK" \
  --publisher "$PUBLISHER_ADDRESS" \
  --json > deployment.json
```

### Scenario 3: Production Deployment with All Details

```bash
soroban-registry contract deploy ./contract.wasm \
  --name "ProductionContract" \
  --description "Production-ready smart contract" \
  --category DeFi \
  --network mainnet \
  --icon ./prod-logo.png \
  --tags "production,stable,audited" \
  --publisher GPUBLIC_KEY_HERE
```

### Scenario 4: Quick Test Deployment

```bash
# Skip ABI extraction for faster deployment
soroban-registry contract deploy ./test_contract.wasm \
  --name "TestContract" \
  --network testnet \
  --skip-abi
```

## Output Interpretation

### Success Response

```
✓ WASM file validation passed
✓ Contract hash computed: a1b2c3d4...
✓ Metadata validation passed
✓ ABI extracted successfully (5 functions found)
✓ Contract registered with ID: 550e8400-e29b-41d4-a716-446655440000

═══════════════════════════════════════════════════
         ✓ CONTRACT DEPLOYMENT SUCCESSFUL
═══════════════════════════════════════════════════

📋 Deployment Details:
  Deployment ID:     550e8400-e29b-41d4-a716-446655440000
  Confirmation Code: DEPLOY-ABC123XYZ789
  Contract Name:     MyContract
  Network:           testnet
  Category:          DeFi
  Verification:      pending
  Created At:        2024-05-28T10:00:00Z

✨ Next steps:
  • Monitor verification at: https://registry.soroban.org/contracts/550e8400-e29b-41d4-a716-446655440000
  • Share your deployment ID: DEPLOY-ABC123XYZ789
```

### Error Response Example

```
✗ Error: Invalid WASM file: incorrect magic bytes. 
  Expected '\0asm' at the start.

💡 Tip: Ensure you're providing a valid compiled WASM file.
```

## Validation Rules

### ✅ WASM File
- Starts with magic bytes: `\0asm` (0x00 0x61 0x73 0x6d)
- File size: 4 bytes minimum, 10 MB maximum
- Must be readable and accessible

### ✅ Contract Name
- Required field
- 1-255 characters
- Cannot be empty

### ✅ Description
- Optional field
- Maximum 5000 characters
- Supports any text content

### ✅ Category
- Optional field
- Must be one of: `DeFi`, `Token`, `Oracle`, `NFT`, `Utility`, `Other`
- Case-sensitive

### ✅ Network
- Required field (default: testnet)
- Must be one of: `mainnet`, `testnet`, `futurenet`
- Case-sensitive

### ✅ Icon
- Optional field
- Supported formats: PNG, JPG, SVG
- Maximum 2 MB
- Verified with magic bytes

### ✅ Tags
- Optional field
- Comma-separated values
- Each tag trimmed of whitespace

### ✅ Publisher
- Optional field
- Stellar address format
- Used for attribution

## Troubleshooting

### "WASM file not found"
```bash
# Solution: Check the file path
ls -la ./my_contract.wasm
soroban-registry contract deploy ./my_contract.wasm ...
```

### "Invalid WASM file: incorrect magic bytes"
```bash
# Solution: Ensure you're using a compiled WASM binary
# Not: source code, JSON, or other files
file ./my_contract.wasm  # Should output "WebAssembly (wasm) binary module"
```

### "Contract name is required"
```bash
# Solution: Add --name parameter or use --interactive
soroban-registry contract deploy ./contract.wasm --name "MyContract" --network testnet
```

### "Icon upload failed"
```bash
# Solution: Check icon file format and size
# - Format must be PNG, JPG, or SVG
# - File size must be < 2 MB
file ./logo.png
du -h ./logo.png
```

### "Icon validation failed: Invalid PNG file"
```bash
# Solution: Verify PNG file integrity
file ./logo.png  # Should output "PNG image data"
```

## Integration Examples

### Bash Script

```bash
#!/bin/bash
WASM_FILE="./contract.wasm"
CONTRACT_NAME="MyContract"
NETWORK="testnet"

soroban-registry contract deploy "$WASM_FILE" \
  --name "$CONTRACT_NAME" \
  --network "$NETWORK" \
  --json | jq '.deployment.id'
```

### GitHub Actions

```yaml
- name: Deploy Contract
  run: |
    soroban-registry contract deploy ./contract.wasm \
      --name "${{ env.CONTRACT_NAME }}" \
      --network testnet \
      --json > deployment.json
    
    DEPLOYMENT_ID=$(jq -r '.deployment.id' deployment.json)
    echo "DEPLOYMENT_ID=$DEPLOYMENT_ID" >> $GITHUB_ENV
```

### CI/CD Pipeline

```bash
#!/bin/bash
set -e

# Build contract
cargo build --target wasm32-unknown-unknown --release

# Deploy
soroban-registry contract deploy \
  "./target/wasm32-unknown-unknown/release/contract.wasm" \
  --name "MyContract" \
  --network "$DEPLOY_NETWORK" \
  --publisher "$PUBLISHER_ADDRESS" \
  --json
```

## Performance Considerations

- **WASM Validation**: < 100ms
- **Hash Computation**: ~500ms for 1MB file
- **ABI Extraction**: 1-5 seconds (depends on soroban CLI availability)
- **Icon Upload**: 500ms - 2 seconds (depends on network)
- **Total Deployment**: 2-10 seconds typically

## Security Notes

1. **WASM Integrity**: Hash is verified and stored
2. **File Validation**: Magic bytes prevent injection
3. **Icon Validation**: Format and size verified
4. **Publisher Attribution**: Address stored with contract
5. **Network Isolation**: Separate namespaces for mainnet/testnet

## Verification After Deployment

```bash
# View deployed contract
soroban-registry contract details <CONTRACT_ID> --network testnet

# Verify contract authenticity
soroban-registry contract verify <CONTRACT_ID> --network testnet

# Check deployment status
curl "https://api.registry.soroban.org/api/contracts/<DEPLOYMENT_ID>"
```

## Support & Feedback

For issues or questions:
1. Check the troubleshooting section above
2. Review the CONTRACT_DEPLOYMENT_IMPLEMENTATION.md for detailed docs
3. Run with `-vv` for debug output: `soroban-registry -vv contract deploy ...`
4. Check GitHub Issues: https://github.com/stellar/soroban-registry/issues

---

**Last Updated:** May 28, 2024
**Version:** 1.0.0 Initial Release
