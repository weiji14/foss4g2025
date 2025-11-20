// Benchmark tests on reading a GeoTIFF into memory (CPU or GPU)
//
// Libraries compared:
// - nvTIFF (Enable NVIDIA network repository and do `sudo apt install nvtiff nvcomp-cuda-12`)
// - GDAL
// - async-tiff
//
// Steps:
// - Download Sentinel-2 True-Colour Image (TCI) file (318.0MB, DEFLATE compression) from
//   https://sentinel-cogs.s3.us-west-2.amazonaws.com/sentinel-s2-l2a-cogs/37/M/BV/2024/10/S2A_37MBV_20241029_0_L2A/TCI.tif
//   to `benches/` folder.
// - Change from DEFLATE to LZW compression with Horizontal differencing predictor (272.2MB)
//   using the following command
//   `gdal raster convert --co COMPRESS=LZW --co TILED=YES --co PREDICTOR=2 benches/TCI.tif benches/TCI_lzw.tif`
// - Run `cargo bench` (CPU-only) or `cargo bench --features cuda` (with CUDA-enabled GPU)
//
// References:
// - https://github.com/microsoft/pytorch-cloud-geotiff-optimization/blob/5fb6d1294163beff822441829dcd63a3791b7808/configs/search.yaml#L6

use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "cuda")]
use std::time::Duration;

use async_tiff::decoder::DecoderRegistry;
use async_tiff::metadata::{PrefetchBuffer, TiffMetadataReader};
use async_tiff::reader::ObjectReader;
use async_tiff::{ImageFileDirectory, Tile};
#[cfg(feature = "cuda")]
use bytes::Bytes;
#[cfg(feature = "cuda")]
use cog3pio::io::nvtiff::CudaCogReader;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
#[cfg(feature = "cuda")]
use cudarc::driver::{CudaContext, CudaStream};
#[cfg(feature = "cuda")]
use dlpark::SafeManagedTensorVersioned;
#[cfg(feature = "cuda")]
use dlpark::traits::TensorView;
use gdal::raster::Buffer;
use gdal::{Dataset, DatasetOptions, GdalOpenFlags};
use ndarray::Array2;
use object_store::path::Path;
use object_store::{ObjectStore, parse_url};
use rayon::ThreadPoolBuilder;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tokio::runtime;
use url::Url;

// nvtiff
#[cfg(feature = "cuda")]
fn read_geotiff_nvtiff(fpath: &str) {
    let v: Vec<u8> = std::fs::read(fpath).unwrap();
    let bytes = Bytes::copy_from_slice(&v);

    let ctx: Arc<CudaContext> = CudaContext::new(0).unwrap(); // Set on GPU:0
    let cuda_stream: Arc<CudaStream> = ctx.default_stream();

    let cog = CudaCogReader::new(&bytes, &cuda_stream).unwrap();
    let tensor: SafeManagedTensorVersioned = cog.dlpack().unwrap();

    assert_eq!(tensor.num_elements(), 3 * 10980 * 10980);
}

// gdal
fn read_geotiff_gdal(fpath: &str, n_threads: usize) {
    let n_threads: String = format!("NUM_THREADS={n_threads}");
    let options = DatasetOptions {
        open_flags: GdalOpenFlags::default(),
        allowed_drivers: Some(&["LIBERTIFF"]),
        open_options: Some(&[n_threads.as_str()]),
        sibling_files: None,
    };
    let ds = Dataset::open_ex(fpath, options).unwrap();

    for b in 1..3 {
        let band = ds.rasterband(b).unwrap();
        let buffer: Buffer<u8> = band.read_band_as::<u8>().unwrap();
        let array: Array2<_> = buffer.to_array().unwrap();

        assert_eq!(array.shape(), [10980, 10980]);

        #[cfg(feature = "cuda")]
        {
            // Copy from CPU (host) memory to CUDA (device) memory
            let ctx: Arc<CudaContext> = CudaContext::new(0).unwrap(); // Set on GPU:0
            let cuda_stream: Arc<CudaStream> = ctx.default_stream();
            let mut cuda_mem = cuda_stream.alloc_zeros::<u8>(3 * 10980 * 10980).unwrap();

            cuda_stream
                .memcpy_htod(array.as_slice().unwrap(), &mut cuda_mem)
                .unwrap();
        }
    }
}

// async-tiff
fn read_geotiff_async_tiff(fpath: &str, n_threads: usize) {
    // let file = File::open(fpath).unwrap();
    let abs_path: PathBuf = std::path::Path::new(fpath).canonicalize().unwrap();
    let tif_url: Url = Url::from_file_path(abs_path).unwrap();
    let (store, path): (Box<dyn ObjectStore>, Path) = parse_url(&tif_url).unwrap();

    let reader = ObjectReader::new(Arc::new(store), path);
    let decoder_registry = DecoderRegistry::default();

    // Initialize async runtime
    let runtime = runtime::Builder::new_current_thread()
        // ::new_multi_thread().worker_threads(n_threads)
        .enable_all()
        .build()
        .unwrap();

    // Get list of tiles in TIFF file stream (using tokio async runtime)
    let tiles: Vec<Tile> = runtime.block_on(async {
        // Read metadata header
        let prefetch_reader = PrefetchBuffer::new(reader.clone(), 32 * 1024)
            .await
            .unwrap();
        let mut metadata_reader = TiffMetadataReader::try_open(&prefetch_reader)
            .await
            .unwrap();

        // Read Image File Directories
        let ifds: Vec<ImageFileDirectory> = metadata_reader
            .read_all_ifds(&prefetch_reader)
            .await
            .unwrap();

        assert_eq!(ifds.len(), 1); // should have only 1 IFD
        let ifd: &ImageFileDirectory = ifds.first().unwrap();

        let (x_count, y_count) = ifd.tile_count().unwrap();
        // dbg!(x_count, y_count); // 43 * 43 = 1849
        // Get cartesian product of x and y tile ids
        let x_ids: Vec<usize> = (0..x_count)
            .flat_map(|i| (0..y_count).map(move |_j| i))
            .collect();
        let y_ids: Vec<usize> = (0..x_count).flat_map(|_i| 0..y_count).collect();

        let tiles: Vec<Tile> = ifd.fetch_tiles(&x_ids, &y_ids, &reader).await.unwrap();
        assert_eq!(tiles.len(), 1849);

        tiles
    });

    // Do actual decoding of TIFF tile data (multi-threaded using rayon)
    let pool = ThreadPoolBuilder::new()
        .num_threads(n_threads)
        .build()
        .unwrap();
    let tile_bytes: Vec<u8> = pool.install(|| {
        tiles
            .into_par_iter()
            .flat_map_iter(|tile| tile.decode(&decoder_registry).unwrap())
            .collect()
    });
    assert_eq!(tile_bytes.len(), 363528192); // should be 361681200, why not?

    #[cfg(feature = "cuda")]
    {
        // Copy from CPU (host) memory to CUDA (device) memory
        let ctx: Arc<CudaContext> = CudaContext::new(0).unwrap(); // Set on GPU:0
        let cuda_stream: Arc<CudaStream> = ctx.default_stream();
        let mut cuda_mem = cuda_stream
            .alloc_zeros::<u8>(
                tile_bytes.len(), // should be 3 * 10980 * 10980 theoretically
            )
            .unwrap();

        cuda_stream.memcpy_htod(&tile_bytes, &mut cuda_mem).unwrap();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_geotiff");

    let fsize: u64 = std::fs::metadata("benches/TCI_lzw.tif").unwrap().len();
    group.throughput(Throughput::BytesDecimal(fsize)); // 272.2MB filesize

    // GPU decoding using nvTIFF
    #[cfg(feature = "cuda")]
    {
        group
            .sample_size(10) // reduce sample size because of CUDA memory leak
            .nresamples(2)
            .warm_up_time(Duration::from_millis(1))
            .measurement_time(Duration::from_secs(2));
        group.bench_with_input(
            BenchmarkId::new("0_nvTIFF_GPU", "Sentinel-2 TCI"),
            "benches/TCI_lzw.tif",
            |b, p| b.iter(|| read_geotiff_nvtiff(p)),
        );
    }

    // CPU decoding using GDAL
    group.sample_size(30);
    group.bench_with_input(
        BenchmarkId::new("1_gdal_CPU_threads=4", "Sentinel-2 TCI"),
        "benches/TCI_lzw.tif",
        |b, p| b.iter(|| read_geotiff_gdal(p, 4)),
    );

    // CPU decoding using async-tiff
    group.sample_size(30);
    group.bench_with_input(
        BenchmarkId::new("2_async-tiff_CPU_threads=4", "Sentinel-2 TCI"),
        "benches/TCI_lzw.tif",
        |b, p| b.iter(|| read_geotiff_async_tiff(p, 4)),
    );

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
