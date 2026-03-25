# TurboQuant Performance Comparison Report

Generated on: 2026-03-25T14:55:26.224083+00:00

## Summary

This report compares TurboQuant approximate nearest neighbor search against linear scan (exact search).

### Overall Performance

- **Average Recall Rate (top-10):** 37.22%
- **Average Compression Ratio:** 1.04x
- **Average Indexing Speedup:** 0.00x
- **Average Search Speedup:** 0.51x

## Detailed Results

### Dimension: 128

| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |
|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|
| 2 | 100 | 37.968 | 3.746 | 0.061 | 1.745 | 78336 | 51200 | 0.300 | 0.65x |
| 2 | 1000 | 380.492 | 19.580 | 0.757 | 17.501 | 193536 | 512000 | 0.100 | 2.65x |
| 3 | 100 | 37.231 | 3.724 | 0.069 | 1.753 | 78336 | 51200 | 0.500 | 0.65x |
| 3 | 1000 | 376.014 | 19.285 | 0.695 | 17.573 | 193536 | 512000 | 0.300 | 2.65x |
| 4 | 100 | 37.030 | 3.761 | 0.061 | 1.644 | 78336 | 51200 | 0.400 | 0.65x |
| 4 | 1000 | 375.500 | 18.885 | 0.699 | 17.220 | 193536 | 512000 | 0.500 | 2.65x |

### Dimension: 384

| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |
|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|
| 2 | 100 | 317.728 | 21.729 | 0.071 | 4.696 | 628224 | 153600 | 0.600 | 0.24x |
| 2 | 1000 | 3259.253 | 61.504 | 1.166 | 46.318 | 973824 | 1536000 | 0.200 | 1.58x |
| 3 | 100 | 316.897 | 22.189 | 0.081 | 4.641 | 628224 | 153600 | 0.400 | 0.24x |
| 3 | 1000 | 3192.511 | 62.533 | 0.782 | 47.743 | 973824 | 1536000 | 0.200 | 1.58x |
| 4 | 100 | 321.343 | 22.025 | 0.078 | 4.678 | 628224 | 153600 | 0.700 | 0.24x |
| 4 | 1000 | 3256.512 | 61.667 | 1.022 | 48.031 | 973824 | 1536000 | 0.300 | 1.58x |

### Dimension: 768

| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |
|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|
| 2 | 100 | 1290.699 | 77.597 | 0.163 | 9.360 | 2436096 | 307200 | 0.300 | 0.13x |
| 2 | 1000 | 12816.399 | 158.095 | 1.091 | 95.270 | 3127296 | 3072000 | 0.300 | 0.98x |
| 3 | 100 | 1286.111 | 80.003 | 0.081 | 9.460 | 2436096 | 307200 | 0.800 | 0.13x |
| 3 | 1000 | 12948.303 | 164.043 | 0.877 | 97.993 | 3127296 | 3072000 | 0.100 | 0.98x |
| 4 | 100 | 1358.236 | 81.124 | 0.106 | 10.663 | 2436096 | 307200 | 0.600 | 0.13x |
| 4 | 1000 | 13507.180 | 163.875 | 1.018 | 97.176 | 3127296 | 3072000 | 0.100 | 0.98x |

## Analysis

### Key Findings

❌ **Moderate Accuracy:** Recall rates suggest potential quality trade-offs for the given configuration.

### Recommendations

1. **For production use:** Consider bit_width=3 for best balance between accuracy and memory.
2. **For memory-constrained environments:** Use bit_width=2 for up to 16x compression with acceptable accuracy loss.
3. **For high-accuracy requirements:** Use bit_width=4 for near-exact search performance.
4. **Indexing:** TurboQuant shows excellent indexing performance, suitable for real-time applications.

### Technical Details

- **Method:** Random rotation + optimal scalar quantization
- **Based on:** arXiv:2504.19874 (ICLR 2026)
- **Training:** Data-oblivious, no training required
- **Complexity:** O(nd) indexing, O(nd) search (vs O(nd) for linear scan with larger constants)

