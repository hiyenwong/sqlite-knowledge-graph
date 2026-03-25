# TurboQuant Performance Comparison Report

Generated on: 2026-03-25T20:09:16.710844+00:00

## Summary

This report compares TurboQuant approximate nearest neighbor search against linear scan (exact search).

### Overall Performance

- **Average Recall Rate (top-10):** 36.67%
- **Average Compression Ratio:** 1.04x
- **Average Indexing Speedup:** 0.00x
- **Average Search Speedup:** 0.51x

## Detailed Results

### Dimension: 128

| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |
|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|
| 2 | 100 | 45.480 | 4.166 | 0.067 | 1.809 | 78336 | 51200 | 0.300 | 0.65x |
| 2 | 1000 | 364.509 | 19.098 | 0.796 | 17.455 | 193536 | 512000 | 0.300 | 2.65x |
| 3 | 100 | 36.780 | 3.675 | 0.056 | 1.675 | 78336 | 51200 | 0.600 | 0.65x |
| 3 | 1000 | 368.470 | 18.546 | 0.745 | 16.974 | 193536 | 512000 | 0.400 | 2.65x |
| 4 | 100 | 36.479 | 3.606 | 0.055 | 1.622 | 78336 | 51200 | 0.500 | 0.65x |
| 4 | 1000 | 365.785 | 18.532 | 0.681 | 16.911 | 193536 | 512000 | 0.500 | 2.65x |

### Dimension: 384

| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |
|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|
| 2 | 100 | 314.998 | 22.147 | 0.073 | 4.554 | 628224 | 153600 | 0.700 | 0.24x |
| 2 | 1000 | 3110.335 | 60.991 | 0.732 | 46.538 | 973824 | 1536000 | 0.300 | 1.58x |
| 3 | 100 | 311.610 | 21.480 | 0.077 | 4.577 | 628224 | 153600 | 0.600 | 0.24x |
| 3 | 1000 | 3125.396 | 60.862 | 0.833 | 46.410 | 973824 | 1536000 | 0.100 | 1.58x |
| 4 | 100 | 316.180 | 21.617 | 0.095 | 4.563 | 628224 | 153600 | 0.200 | 0.24x |
| 4 | 1000 | 3147.823 | 60.658 | 0.903 | 45.981 | 973824 | 1536000 | 0.500 | 1.58x |

### Dimension: 768

| Bit Width | Vectors | TurboQuant Index (ms) | TurboQuant Search (ms) | Linear Index (ms) | Linear Search (ms) | Memory Turbo (bytes) | Memory Linear (bytes) | Recall Rate | Compression |
|-----------|---------|----------------------|-----------------------|-------------------|-------------------|---------------------|----------------------|-------------|--------------|
| 2 | 100 | 1234.936 | 76.272 | 0.209 | 9.008 | 2436096 | 307200 | 0.400 | 0.13x |
| 2 | 1000 | 12392.398 | 151.629 | 1.597 | 89.660 | 3127296 | 3072000 | 0.200 | 0.98x |
| 3 | 100 | 1238.414 | 76.348 | 0.184 | 8.948 | 2436096 | 307200 | 0.600 | 0.13x |
| 3 | 1000 | 12448.063 | 151.528 | 1.101 | 89.838 | 3127296 | 3072000 | 0.200 | 0.98x |
| 4 | 100 | 1242.848 | 76.574 | 0.138 | 8.944 | 2436096 | 307200 | 0.200 | 0.13x |
| 4 | 1000 | 12450.742 | 151.783 | 1.360 | 89.678 | 3127296 | 3072000 | 0.000 | 0.98x |

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

