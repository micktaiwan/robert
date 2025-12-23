# Robert - Development Difficulties

## Tauri 2

### White/Blank Windows
- **Symptom**: Overlay and settings windows showed blank white content
- **Root Cause**: Running `./target/release/robert` directly instead of `npm run tauri dev`
- **Explanation**: When running the binary directly without Tauri's build system, it tries to load from `devUrl` (localhost:5173) but no Vite server is running
- **Solution**: Always use `npm run tauri dev` for development or `npm run tauri build` for production

### Transparent Windows
- **Symptom**: `.transparent(true)` method doesn't exist on `WebviewWindowBuilder`
- **Root Cause**: Tauri 2 requires `macOSPrivateApi` feature for transparent windows
- **Status**: Not implemented, using opaque dark background instead

## LLM Integration

### llama-cpp-2 Crate
- **Symptom**: "tensor duplicated" errors during inference
- **Root Cause**: Unknown issue with the llama-cpp-2 Rust bindings
- **Solution**: Migrated to Candle framework

### Candle with Metal (GPU)
- **Symptom**: `Metal error no metal implementation for rms-norm`
- **Root Cause**: Candle's Metal backend doesn't support the RMS normalization operation used by Qwen2.5
- **Solution**: Fall back to CPU with `Device::Cpu`

### Candle CPU Shape Mismatch
- **Symptom**: `shape mismatch in broadcast_add, lhs: [1, 14, 94, 94], rhs: [1, 1, 94, 188]`
- **Root Cause**: Incorrect handling of KV cache during token generation - passing all tokens with wrong position offset
- **Solution**: Fixed generation loop to pass only new token after first iteration with correct position

### LLM Inference Speed
- **Symptom**: 24+ seconds for a single classification on CPU
- **Root Cause**: Qwen2.5-0.5B (500M parameters) is too large for fast CPU inference with Candle
- **Status**: Unresolved - considering keyword matching for simple commands instead

## Whisper

### Verbose Logs
- **Symptom**: whisper.cpp prints excessive logs to stderr (model loading, inference details)
- **Solution**: Redirect stderr to `/dev/null` during Whisper calls using `libc::dup2`
