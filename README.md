# [FOSS4G 2025 presentation](https://talks.osgeo.org/foss4g-2025/talk/MRPVGL)

Accelerating GeoTIFF readers with Rust.

**Presenter**: [Wei Ji Leong](https://github.com/weiji14)

**When**: [Thursday, 20 November 2025, 16:30–16:55 (NZDT)](https://2025.foss4g.org/program/schedule#event-4234-accelerating-geotiff-readers-with-rust)

**Where**: [WG308 TE IRINGA, Auckland University of Technology (AUT)](https://2025.foss4g.org/attend/conference-venue), Tāmaki Makaurau / Auckland, Aotearoa / New Zealand

**Website**: https://talks.osgeo.org/foss4g-2025/talk/MRPVGL

## Abstract

Reading a Cloud-optimized GeoTIFF involves several steps, from fetching compressed bytes
over a network/disk, decompressing those bytes, to finally parsing of TIFF tag metadata.
Can we speed up the decoding using asynchronous methods, or even GPU-accelerated
libraries? Let's see how we can program this in Rust!

### Long description

How can we compose together a modern library to decode Cloud-optimized GeoTIFFs (COGs)
efficiently? By using a programming language called Rust, with bindings to Python,
WebAssembly and more, our goal is to enable applications that demand high-performance
reads, such as web-based COG tilers or machine learning workflows leveraging Graphical
Processing Units (GPUs). For CPU workflows, we delegate the network/disk transfer
handling to the [`object_store`](https://crates.io/crates/object_store) crate, use
various Rust-based algorithms for decompressing raw bytes, and let the
[`async-tiff`](https://crates.io/crates/async-tiff) crate do the actual TIFF tag
metadata and pixel data parsing. For GPU workflows, we swap the decompression library
for [`nvCOMP`](https://developer.nvidia.com/nvcomp), and do the TIFF parsing using
[`nvTIFF`](https://developer.nvidia.com/nvtiff), with the resulting pixel data decoded
directly into CUDA device memory. Come and see how these asynchronous and
GPU-accelerated GeoTIFF readers compare against GDAL's
[`libertiff`](https://gdal.org/en/release-3.11/drivers/raster/libertiff.html) driver,
and find out how we're making these performant low-level Rust-based readers more
accessible by integrating with the [xarray](https://xarray.dev) ecosystem and beyond!

## License

All code in this repository is licensed under
Mozilla Public License Version 2.0 [(MPL-2.0)](https://www.mozilla.org/en-US/MPL/2.0/).
All other non-code content is licensed under
Creative Commons Attribution-ShareAlike 4.0 International
[(CC BY-SA 4.0)](https://creativecommons.org/licenses/by-sa/4.0
