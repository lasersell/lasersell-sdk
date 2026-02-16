package lasersell

import (
	"net"
	"net/http"
	"time"
)

// Ptr returns a pointer to value.
func Ptr[T any](value T) *T {
	return &value
}

func newNoProxyHTTPClient(connectTimeout time.Duration) *http.Client {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.Proxy = nil
	if connectTimeout > 0 {
		dialer := &net.Dialer{
			Timeout:   connectTimeout,
			KeepAlive: 30 * time.Second,
		}
		transport.DialContext = dialer.DialContext
	}

	return &http.Client{Transport: transport}
}
