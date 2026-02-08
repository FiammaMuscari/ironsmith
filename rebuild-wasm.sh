wasm-pack build --release --target web --features wasm
mkdir -p /Users/chiplis/ironsmith/web/wasm_demo/pkg
cp -f /Users/chiplis/ironsmith/pkg/ironsmith.js \
      /Users/chiplis/ironsmith/pkg/ironsmith_bg.wasm \
      /Users/chiplis/ironsmith/pkg/ironsmith.d.ts \
      /Users/chiplis/ironsmith/pkg/ironsmith_bg.wasm.d.ts \
      /Users/chiplis/ironsmith/pkg/package.json \
      /Users/chiplis/ironsmith/web/wasm_demo/pkg/
