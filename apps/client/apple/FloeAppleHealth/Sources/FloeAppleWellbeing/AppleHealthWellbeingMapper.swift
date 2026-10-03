import FloeAppleHealth
import FloeHealthTransform

public enum AppleHealthWellbeingMapper {
    public static func map(_ acquisition: HealthKitWellbeingAcquisition) throws -> HealthTransformInput {
        // The acquisition API already reports hours, counts, and minutes respectively.
        let input = try HealthTransformInput(
            sleepHours: acquisition.sleepHours,
            steps: acquisition.steps,
            exerciseMinutes: acquisition.exerciseMinutes
        )
        try input.validate()
        return input
    }
}
