#!/bin/bash
# Generate Python gRPC stubs from proto files

set -e

cd "$(dirname "$0")"

echo "🔨 Generating Python gRPC stubs..."

python -m grpc_tools.protoc \
    -I../proto \
    --python_out=polaroid \
    --grpc_python_out=polaroid \
    ../proto/polaroid.proto

echo "✅ Generated polaroid_pb2.py and polaroid_pb2_grpc.py"

# Fix imports in generated files (Python 3.9+ compatibility)
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sed -i '' 's/import polaroid_pb2/from . import polaroid_pb2/g' polaroid/polaroid_pb2_grpc.py
else
    # Linux
    sed -i 's/import polaroid_pb2/from . import polaroid_pb2/g' polaroid/polaroid_pb2_grpc.py
fi

echo "✅ Fixed imports"
echo "🎉 Done!"
