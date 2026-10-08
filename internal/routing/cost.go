package routing

import (
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/provider"
)

type ModelPrice struct {
	InputPerMillion  float64
	OutputPerMillion float64
}

// DefaultPricing provides token pricing per 1M tokens in USD.
var DefaultPricing = map[string]ModelPrice{
	"gpt-4o": {
		InputPerMillion:  2.50,
		OutputPerMillion: 10.00,
	},
	"gpt-4o-mini": {
		InputPerMillion:  0.15,
		OutputPerMillion: 0.60,
	},
	"claude-3-5-sonnet": {
		InputPerMillion:  3.00,
		OutputPerMillion: 15.00,
	},
	"claude-3-5-haiku": {
		InputPerMillion:  0.80,
		OutputPerMillion: 4.00,
	},
	"mock": {
		InputPerMillion:  0.0,
		OutputPerMillion: 0.0,
	},
}

// CalculateCost calculates the USD cost of a model turn based on token usage.
func CalculateCost(model string, usage provider.Usage) float64 {
	price, ok := DefaultPricing[model]
	if !ok {
		// Clean up common prefixes like "openai: gpt-4o"
		clean := strings.ToLower(model)
		for k, p := range DefaultPricing {
			if strings.Contains(clean, k) {
				price = p
				ok = true
				break
			}
		}
	}
	if !ok {
		// Default to gpt-4o-mini pricing if unknown
		price = DefaultPricing["gpt-4o-mini"]
	}

	inputCost := (float64(usage.PromptTokens) / 1_000_000.0) * price.InputPerMillion
	outputCost := (float64(usage.CompletionTokens) / 1_000_000.0) * price.OutputPerMillion
	return inputCost + outputCost
}

// FormatCost formats a float USD cost into human-readable representation.
func FormatCost(cost float64) string {
	if cost == 0 {
		return "$0.00"
	}
	if cost < 0.01 {
		return fmt.Sprintf("$%.4f", cost)
	}
	return fmt.Sprintf("$%.2f", cost)
}
