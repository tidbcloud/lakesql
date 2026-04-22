const path = require('path')
const webpack = require('webpack')

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  trailingSlash: false,
  output: 'export',
  distDir: 'build',
  images: {
    unoptimized: true
  },
  // Generate fixed filenames to reduce git changes
  webpack: (config, { isServer }) => {
    // `@antv/l7-component` (transitively pulled in by `@ant-design/charts`)
    // ships raw `.less` files that Next.js does not compile. The imports
    // come from the map stack, which our flow-graph usage doesn't render,
    // so redirect every `.less` import to an empty stub. Done via a
    // plugin instead of a `module.rules` entry so Next.js's built-in CSS
    // handling stays enabled.
    config.plugins.push(
      new webpack.NormalModuleReplacementPlugin(
        /\.less$/,
        path.resolve(__dirname, 'empty-less.js')
      )
    )

    if (!isServer) {
      // Use fixed filenames instead of content hashes
      config.output.filename = 'static/js/[name].js'
      config.output.chunkFilename = 'static/js/[name].js'
      
      // Ensure CSS filenames are also fixed
      const miniCssExtractPlugin = config.plugins.find(
        plugin => plugin.constructor.name === 'MiniCssExtractPlugin'
      )
      if (miniCssExtractPlugin) {
        miniCssExtractPlugin.options.filename = 'static/css/[name].css'
        miniCssExtractPlugin.options.chunkFilename = 'static/css/[name].css'
      }

      // Disable dynamic chunk naming in code splitting
      config.optimization = {
        ...config.optimization,
        splitChunks: {
          ...config.optimization.splitChunks,
          cacheGroups: {
            ...config.optimization.splitChunks.cacheGroups,
            default: false,
            vendors: false,
            // Merge all vendor code into one file
            vendor: {
              name: 'vendor',
              chunks: 'all',
              test: /node_modules/,
              filename: 'static/js/vendor.js'
            },
            // Merge all shared code
            common: {
              name: 'common',
              chunks: 'all',
              minChunks: 2,
              filename: 'static/js/common.js'
            }
          }
        }
      }
    }
    return config
  },
  // Ensure build ID is stable
  generateBuildId: async () => {
    return 'stable-build-id'
  }
}

module.exports = nextConfig