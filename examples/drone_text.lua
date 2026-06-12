local LABELS = {
    AHRS = "Quaternion",
    YPR  = "Yaw/Pitch/Roll",
    Gyro = "Gyro (deg/s)",
    RC   = "RC Channels",
    M    = "Motors",
    L    = "L",
    F    = "F",
    C    = "C",
}

return {
    decode = function(frame)
        if #frame == 0 then return nil end

        local lines = {}
        for segment in frame:gmatch("[^|]+") do
            local colon = segment:find(":")
            if colon then
                local key = segment:sub(1, colon - 1)
                local val = segment:sub(colon + 1)
                local label = LABELS[key] or key
                lines[#lines + 1] = string.format("%-18s %s", label .. ":", val)
            end
        end
        lines[#lines + 1] = string.rep("-", 40)

        return {
            kind = "text",
            data = table.concat(lines, "\n"),
        }
    end,
}
