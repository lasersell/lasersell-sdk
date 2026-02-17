package stream

import "testing"

func TestStreamClientUsesProductionEndpointByDefault(t *testing.T) {
	client := NewStreamClient("test-api-key")
	if got := client.endpoint(); got != StreamEndpoint {
		t.Fatalf("expected production endpoint %q, got %q", StreamEndpoint, got)
	}
}

func TestStreamClientUsesLocalEndpointWhenEnabled(t *testing.T) {
	client := NewStreamClient("test-api-key").WithLocalMode(true)
	if got := client.endpoint(); got != LocalStreamEndpoint {
		t.Fatalf("expected local endpoint %q, got %q", LocalStreamEndpoint, got)
	}
}

func TestStreamClientEndpointOverrideTakesPrecedence(t *testing.T) {
	client := NewStreamClient("test-api-key").
		WithLocalMode(true).
		WithEndpoint("wss://stream-dev.example/ws   \n")

	if got := client.endpoint(); got != "wss://stream-dev.example/ws" {
		t.Fatalf("expected endpoint override to be trimmed and preferred, got %q", got)
	}
}
