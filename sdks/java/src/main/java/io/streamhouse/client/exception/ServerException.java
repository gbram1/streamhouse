package io.streamhouse.client.exception;

public class ServerException extends StreamHouseException {
    public ServerException(String message, int statusCode) {
        super(message, statusCode);
    }
}
