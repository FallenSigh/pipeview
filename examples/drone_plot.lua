return {
    decode = function(frame)
        if #frame == 0 then return nil end

        local gyro_str = frame:match("Gyro:([%d%.%-]+),([%d%.%-]+),([%d%.%-]+)")
        if not gyro_str then return nil end

        local gz, gy, gx = frame:match("Gyro:([%d%.%-]+),([%d%.%-]+),([%d%.%-]+)")
        return {
            kind = "plot",
            channels = { { tonumber(gz) }, { tonumber(gy) }, { tonumber(gx) } },
            sample_type = "F64",
            format = "Block",
        }
    end,
}
