# 🚀 Rust PDF Generator (Chromium + GS Compression)

**Blazing-fast PDF generation with Rust, Chromium, and Ghostscript compression — perfect for AWS Lambda!**

![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust) ![Chromium](https://img.shields.io/badge/Chromium-4285F4?style=flat&logo=google-chrome) ![AWS Lambda](https://img.shields.io/badge/AWS%20Lambda-FF9900?style=flat&logo=amazon-aws)

---

## 💡 Overview

This project is a **high-performance PDF generator** built in **Rust**, leveraging **headless Chromium** for rendering and **Ghostscript (GS)** for PDF compression.  

It's designed to be **faster and more memory-efficient than Node.js + Chromium solutions** and works seamlessly on **AWS Lambda**, making it ideal for serverless PDF generation at scale.

---

## ⚡ Key Features

- 🖥 **Chromium Rendering**: Full HTML → PDF support and PNG support.  
- 🗜 **GS Compression**: Reduce PDF file size efficiently.  
- 🚀 **Faster than Node.js**: Lower memory footprint (~300MB vs ~2000MB in Puppeteer).  
- ☁️ **Lambda-ready**: Runs on AWS Lambda with minimal setup.  
- 🦀 **Rust-powered**: Safe, fast, and minimal dependencies.  

---

## 📦 Download Prebuilt Layer / Binary

Download the prebuilt Lambda layer here: [lambda-layer.zip](https://github.com/jsaurav7/url2pdf/raw/refs/heads/master/layer.zip)

Download the Lambda artifact here: [url2pdf](https://github.com/jsaurav7/url2pdf/actions/runs/17383301791/artifacts/3900026166)

## ☁️ Running on Lambda

To run the Rust PDF generator on AWS Lambda, you need to **attach the provided Lambda layer** and set the required environment variables.

### 1️⃣ Attach the Layer
- Use the layer ZIP file we provide (contains Chromium, fonts, and libraries).  
- In the AWS Lambda console, go to **Layers → Add a layer → Specify an ARN**.  
- Attach it to your Lambda function.

### 2️⃣ Set Environment Variables
Set the following environment variables in your Lambda function configuration:

```bash
LD_LIBRARY_PATH=/opt/al2023/lib:/var/lang/lib:/lib64:/usr/lib64:/var/runtime:/var/runtime/lib:/var/task:/var/task/lib:/opt/lib
FONTCONFIG_PATH=/opt/fonts
