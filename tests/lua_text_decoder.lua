return {
    decode = function(frame)
        if #frame == 0 then
            return nil
        end

        return {
            kind = "text",
            data = frame,
        }
    end,
}
