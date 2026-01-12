#!/bin/bash
# Start polaroid-grpc server with Python dylib path
export DYLD_LIBRARY_PATH=$(python3 -c "import sys; print(sys.prefix + '/lib')"):$DYLD_LIBRARY_PATH
echo "Starting polaroid-grpc server on port 50051..."
exec /Users/melvinalvarez/Documents/Workspace/polaroid/target/debug/polaroid-grpc
