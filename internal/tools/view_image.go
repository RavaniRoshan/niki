package tools

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"image"
	_ "image/gif"
	"image/jpeg"
	"image/png"
	"os"
	"strings"
)

const (
	maxDimension = 1024
)

type ViewImageTool struct {
	Base
}

func NewViewImageTool() *ViewImageTool {
	return &ViewImageTool{
		Base: Base{
			SchemaStr: `{"required":["path"],"fields":{"path":"string","detail":"string"}}`,
		},
	}
}

func (t *ViewImageTool) Name() string { return "view_image" }
func (t *ViewImageTool) Description() string {
	return "Inspect and decode an image file (PNG/JPEG/GIF) into a base64 data URL formatted for vision models"
}

type viewImageArgs struct {
	Path   string `json:"path"`
	Detail string `json:"detail,omitempty"` // "low" | "high" | "original"
}

// resizeDownscales image to maxDim using nearest-neighbor sampling.
func resizeDownscale(img image.Image, maxDim int) image.Image {
	bounds := img.Bounds()
	w := bounds.Dx()
	h := bounds.Dy()

	if w <= maxDim && h <= maxDim {
		return img
	}

	var newW, newH int
	if w > h {
		newW = maxDim
		newH = int(float64(h) * float64(maxDim) / float64(w))
	} else {
		newH = maxDim
		newW = int(float64(w) * float64(maxDim) / float64(h))
	}
	if newW < 1 {
		newW = 1
	}
	if newH < 1 {
		newH = 1
	}

	dst := image.NewRGBA(image.Rect(0, 0, newW, newH))
	for y := 0; y < newH; y++ {
		srcY := bounds.Min.Y + int(float64(y)*float64(h)/float64(newH))
		for x := 0; x < newW; x++ {
			srcX := bounds.Min.X + int(float64(x)*float64(w)/float64(newW))
			dst.Set(x, y, img.At(srcX, srcY))
		}
	}
	return dst
}

func (t *ViewImageTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a viewImageArgs
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}

	p := strings.TrimSpace(a.Path)
	if p == "" {
		return ToolResult{Output: "path cannot be empty", IsError: true}, nil
	}

	f, err := os.Open(p)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("cannot open image file %s: %v", p, err), IsError: true}, nil
	}
	defer f.Close()

	img, format, err := image.Decode(f)
	if err != nil {
		return ToolResult{Output: fmt.Sprintf("unsupported or corrupt image format for %s: %v (supported: png, jpeg, gif)", p, err), IsError: true}, nil
	}

	origBounds := img.Bounds()
	origW, origH := origBounds.Dx(), origBounds.Dy()

	finalImg := img
	resized := false
	if a.Detail != "original" && (origW > maxDimension || origH > maxDimension) {
		finalImg = resizeDownscale(img, maxDimension)
		resized = true
	}

	var buf bytes.Buffer
	var mimeType string
	switch format {
	case "jpeg":
		mimeType = "image/jpeg"
		err = jpeg.Encode(&buf, finalImg, &jpeg.Options{Quality: 85})
	default:
		// Default to PNG encoding for GIF / PNG
		mimeType = "image/png"
		err = png.Encode(&buf, finalImg)
	}

	if err != nil {
		return ToolResult{Output: fmt.Sprintf("failed encoding image %s: %v", p, err), IsError: true}, nil
	}

	encoded := base64.StdEncoding.EncodeToString(buf.Bytes())
	dataURL := fmt.Sprintf("data:%s;base64,%s", mimeType, encoded)

	finalBounds := finalImg.Bounds()
	estTokens := (finalBounds.Dx() * finalBounds.Dy()) / 750
	if estTokens < 85 {
		estTokens = 85
	}

	meta := fmt.Sprintf("Image loaded: %s\nOriginal Dimensions: %dx%d (%s)\nProcessed Dimensions: %dx%d (resized: %v)\nSize: %d bytes\nEstimated Tokens: ~%d\nData URL: %s",
		p, origW, origH, format, finalBounds.Dx(), finalBounds.Dy(), resized, buf.Len(), estTokens, dataURL[:min(60, len(dataURL))]+"...")

	return ToolResult{Output: meta}, nil
}

func (t *ViewImageTool) IsConcurrencySafe() bool { return true }
func (t *ViewImageTool) IsReadOnly() bool        { return true }
