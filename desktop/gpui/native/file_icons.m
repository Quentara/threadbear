#import <AppKit/AppKit.h>
#import <ImageIO/ImageIO.h>
#include <math.h>
#include <unistd.h>

static uint32_t tb_big32(const uint8_t *p) {
  return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
}

// Called only by open_gate.rs. File icons use ImageIO/CoreGraphics on a
// worker; the gate hands over an application's NSImage on the main thread.
int tb_file_icon_png(int fd, long long length, void *applicationIcon, void **output) {
  @autoreleasepool {
    *output = NULL;
    CGImageRef image = NULL;
    CGPDFDocumentRef pdf = NULL;
    if (applicationIcon) {
      NSImage *icon = (__bridge NSImage *)applicationIcon;
      NSRect rect = NSMakeRect(0, 0, 128, 128);
      image = [icon CGImageForProposedRect:&rect context:nil hints:nil];
      if (image) CGImageRetain(image);
    } else if (fd >= 0 && length > 0 && length <= 32LL * 1024 * 1024) {
      NSMutableData *bytes = [NSMutableData dataWithLength:(NSUInteger)length];
      long long at = 0;
      while (at < length) {
        ssize_t got = pread(fd, (char *)bytes.mutableBytes + at, (size_t)(length - at), at);
        if (got <= 0) break;
        at += got;
      }
      if (at != length) return 0;
      if (length >= 5 && memcmp(bytes.bytes, "%PDF-", 5) == 0) {
        CGDataProviderRef provider = CGDataProviderCreateWithCFData((__bridge CFDataRef)bytes);
        pdf = CGPDFDocumentCreateWithProvider(provider);
        CGDataProviderRelease(provider);
      } else {
        // Validate the stable byte copy too, in case the held source was
        // rewritten after the gate read its header.
        if (length < 8 || memcmp(bytes.bytes, "icns", 4) != 0 || tb_big32((const uint8_t *)bytes.bytes + 4) != length) return 0;
        CGImageSourceRef source = CGImageSourceCreateWithData((__bridge CFDataRef)bytes, NULL);
        if (source) {
          // An icns can list its 16 px representation first. Decode the
          // largest one so a Retina rail doesn't magnify that small image.
          size_t best = SIZE_MAX;
          double largest = 0;
          for (size_t i = 0; i < CGImageSourceGetCount(source); i++) {
            NSDictionary *properties = CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, i, NULL));
            double width = [properties[(id)kCGImagePropertyPixelWidth] doubleValue];
            double height = [properties[(id)kCGImagePropertyPixelHeight] doubleValue];
            // ImageIO reports decoded-image dimensions, rather than the
            // icns type code (checked with a 4096 px PNG in an ic10 entry).
            // Refuse oversized images before asking the codec to decode.
            if (!(width > 0 && height > 0 && width <= 2048 && height <= 2048)) continue;
            double area = width * height;
            if (area > largest) { largest = area; best = i; }
          }
          if (best != SIZE_MAX) image = CGImageSourceCreateThumbnailAtIndex(source, best, (__bridge CFDictionaryRef)@{
            (id)kCGImageSourceCreateThumbnailFromImageAlways: @YES,
            (id)kCGImageSourceThumbnailMaxPixelSize: @128,
          });
          CFRelease(source);
        }
      }
    }
    CGPDFPageRef page = pdf ? CGPDFDocumentGetPage(pdf, 1) : NULL;
    if (!image && !page) { if (pdf) CGPDFDocumentRelease(pdf); return 0; }
    CGColorSpaceRef color = CGColorSpaceCreateDeviceRGB();
    CGContextRef context = CGBitmapContextCreate(NULL, 128, 128, 8, 0, color, (CGBitmapInfo)kCGImageAlphaPremultipliedLast);
    CGColorSpaceRelease(color);
    if (!context) { if (image) CGImageRelease(image); if (pdf) CGPDFDocumentRelease(pdf); return 0; }
    // Explicitly clear the padding around an icon of a different shape.
    CGContextClearRect(context, CGRectMake(0, 0, 128, 128));
    if (page) {
      CGRect box = CGRectIntersection(CGPDFPageGetBoxRect(page, kCGPDFCropBox), CGPDFPageGetBoxRect(page, kCGPDFMediaBox));
      if (CGRectIsEmpty(box) || CGRectIsInfinite(box) || CGRectIsNull(box)) box = CGPDFPageGetBoxRect(page, kCGPDFMediaBox);
      if (CGRectIsEmpty(box) || CGRectIsInfinite(box) || CGRectIsNull(box)) {
        CGContextRelease(context); CGPDFDocumentRelease(pdf); return 0;
      }
      int rotation = ((CGPDFPageGetRotationAngle(page) % 360) + 360) % 360;
      BOOL sideways = rotation == 90 || rotation == 270;
      double scale = MIN(128. / (sideways ? box.size.height : box.size.width), 128. / (sideways ? box.size.width : box.size.height));
      // CGPDFPageGetDrawingTransform only scales down. Small vector icons
      // need to grow too, while respecting the page's crop and rotation.
      CGContextTranslateCTM(context, 64, 64);
      CGContextScaleCTM(context, scale, scale);
      CGContextRotateCTM(context, -rotation * M_PI / 180.);
      CGContextTranslateCTM(context, -CGRectGetMidX(box), -CGRectGetMidY(box));
      CGContextClipToRect(context, box);
      CGContextDrawPDFPage(context, page);
    } else {
      double scale = MIN(128. / CGImageGetWidth(image), 128. / CGImageGetHeight(image));
      CGSize size = CGSizeMake(CGImageGetWidth(image) * scale, CGImageGetHeight(image) * scale);
      CGContextDrawImage(context, CGRectMake((128 - size.width) / 2, (128 - size.height) / 2, size.width, size.height), image);
    }
    CGImageRef rendered = CGBitmapContextCreateImage(context);
    CGContextRelease(context);
    if (image) CGImageRelease(image);
    if (pdf) CGPDFDocumentRelease(pdf);
    if (!rendered) return 0;
    NSMutableData *png = [NSMutableData data];
    CGImageDestinationRef destination = CGImageDestinationCreateWithData((__bridge CFMutableDataRef)png, CFSTR("public.png"), 1, NULL);
    BOOL encoded = NO;
    if (destination) {
      CGImageDestinationAddImage(destination, rendered, NULL);
      encoded = CGImageDestinationFinalize(destination);
      CFRelease(destination);
    }
    CGImageRelease(rendered);
    if (!encoded || png.length > INT_MAX) return 0;
    void *copy = malloc(png.length);
    if (!copy) return 0;
    memcpy(copy, png.bytes, png.length);
    *output = copy;
    return (int)png.length;
  }
}
