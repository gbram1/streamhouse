package io.streamhouse.client.exception;

public class StreamHouseException extends Exception {
    private final int statusCode;

    public StreamHouseException(String message) {
        super(message);
        this.statusCode = 0;
    }

    public StreamHouseException(String message, int statusCode) {
        super(message);
        this.statusCode = statusCode;
    }

    public StreamHouseException(String message, Throwable cause) {
        super(message, cause);
        this.statusCode = 0;
    }

    public int getStatusCode() {
        return statusCode;
    }
}
