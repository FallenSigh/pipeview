local buffer = ""

return {
    feed = function(bytes)
        buffer = buffer .. bytes
        local frames = {}

        while true do
            local i = buffer:find("\n", 1, true)
            if not i then
                break
            end

            local frame = buffer:sub(1, i - 1)
            if frame:sub(-1) == "\r" then
                frame = frame:sub(1, -2)
            end

            frames[#frames + 1] = frame
            buffer = buffer:sub(i + 1)
        end

        return frames
    end,

    flush = function()
        if #buffer == 0 then
            return nil
        end

        local frame = buffer
        buffer = ""
        return frame
    end,

    reset = function()
        buffer = ""
    end,

    pending_len = function()
        return #buffer
    end,
}
