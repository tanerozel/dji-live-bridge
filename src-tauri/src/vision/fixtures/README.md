# ONNX regression fixture

`matmul-add.onnx` is a hand-encoded, 240-byte ONNX protobuf. It uses IR version 8
and the default-domain operator set version 17, with no external data or extra
dependencies needed to create or load it.

The graph computes `Y = MatMul(X, W) + B` using float32 tensors:

- `X`: one input, shape `[1, 2]`.
- `W`: row-major initializer `[[1, 2], [3, 4]]`, shape `[2, 2]`.
- `B`: initializer `[1, 2]`, shape `[2]`.
- `T`: the intermediate matrix product, shape `[1, 2]`.
- `Y`: one output, shape `[1, 2]`.

For `X = [[5, 6]]`, the expected output is `[[24, 36]]`. For `X = [[1, 1]]`,
the expected output is `[[5, 8]]`.

Its purpose is to verify the MatMul/Add fusion policy, numerical behavior and
folded-cache migration on the CPU. The large CoreML weight-storage behavior and
model-loading performance require the separate real-model benchmark.
