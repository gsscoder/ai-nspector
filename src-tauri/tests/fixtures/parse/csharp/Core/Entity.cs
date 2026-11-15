namespace Acme.Core;

public class Entity { }

public record Order(int Id);

public enum Status { Open, Closed }
