import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import * as esbuild from 'esbuild';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function build() {
    const distDir = path.resolve(__dirname, 'dist');
    if (!fs.existsSync(distDir)) {
        fs.mkdirSync(distDir, { recursive: true });
    }

    console.log('Building web receiver bundle...');
    await esbuild.build({
        entryPoints: [path.resolve(__dirname, 'app.js')],
        bundle: true,
        format: 'esm',
        outfile: path.resolve(distDir, 'app.js'),
        target: 'es2022',
        minify: true,
    });

    console.log('Copying static assets (index.html, style.css)...');
    fs.copyFileSync(
        path.resolve(__dirname, 'index.html'),
        path.resolve(distDir, 'index.html')
    );
    fs.copyFileSync(
        path.resolve(__dirname, 'style.css'),
        path.resolve(distDir, 'style.css')
    );

    console.log('Build completed successfully -> web-receiver/dist');
}

build().catch(err => {
    console.error('Build failed:', err);
    process.exit(1);
});
